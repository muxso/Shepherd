use crate::domain::ssh::{
    SshAction, SshAuthentication, SshOutput, SshReport, SshTarget, SshTermination,
};
use russh::{
    client,
    keys::{
        ssh_key::{Fingerprint, Kdf},
        HashAlg, PrivateKey, PrivateKeyWithHashAlg, PublicKey,
    },
    ChannelMsg,
};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
#[derive(Debug, thiserror::Error)]
pub enum SshError {
    #[error("invalid SSH host, port, or username")]
    InvalidTarget,
    #[error("a valid SHA256 SSH host fingerprint is required")]
    InvalidFingerprint,
    #[error("SSH host key does not match the configured fingerprint")]
    HostKeyMismatch,
    #[error("SSH authentication failed")]
    Authentication,
    #[error("invalid or unsupported OpenSSH private key/passphrase")]
    PrivateKey,
    #[error("SSH private key derivation exceeds supported cost")]
    KeyCost,
    #[error("SSH command is empty or too large")]
    InvalidCommand,
    #[error("invalid SSH execution budget")]
    InvalidBudget,
    #[error("SSH operation timed out")]
    Timeout,
    #[error("SSH result exceeds output limit")]
    OutputLimit,
    #[error("SSH server rejected the execution request")]
    ExecRejected,
    #[error("unsupported SSH extended-data stream")]
    ExtendedStream,
    #[error("SSH server sent conflicting termination messages")]
    Termination,
    #[error("SSH operation failed: {0}")]
    Transport(#[source] russh::Error),
    #[error("SSH socket failed: {0}")]
    Io(#[source] std::io::Error),
}
impl From<russh::Error> for SshError {
    fn from(error: russh::Error) -> Self {
        match error {
            russh::Error::ConnectionTimeout
            | russh::Error::KeepaliveTimeout
            | russh::Error::InactivityTimeout => Self::Timeout,
            russh::Error::IO(error) if error.kind() == std::io::ErrorKind::TimedOut => {
                Self::Timeout
            }
            error => Self::Transport(error),
        }
    }
}
impl From<std::io::Error> for SshError {
    fn from(error: std::io::Error) -> Self {
        if error.kind() == std::io::ErrorKind::TimedOut {
            Self::Timeout
        } else {
            Self::Io(error)
        }
    }
}
#[derive(Clone, Copy)]
pub struct SshBudget {
    pub timeout_ms: u64,
    pub max_bytes: usize,
}
pub fn validate_target(target: &SshTarget) -> Result<(), SshError> {
    if target.host.trim().is_empty()
        || target.host.len() > 253
        || target.port == 0
        || target.username.trim().is_empty()
        || target.username.len() > 128
    {
        return Err(SshError::InvalidTarget);
    }
    fingerprint(target).map(|_| ())
}
fn fingerprint(target: &SshTarget) -> Result<Fingerprint, SshError> {
    let value =
        target.host_key_sha256.parse::<Fingerprint>().map_err(|_| SshError::InvalidFingerprint)?;
    if !matches!(value, Fingerprint::Sha256(_)) {
        return Err(SshError::InvalidFingerprint);
    }
    Ok(value)
}
// A duplicated socket descriptor lets Drop close transport even if a library task is in key exchange.
struct OwnedSocket(std::net::TcpStream);
impl Drop for OwnedSocket {
    fn drop(&mut self) {
        let _ = self.0.shutdown(std::net::Shutdown::Both);
    }
}
struct PinnedHost {
    expected: Fingerprint,
    rejected: Arc<AtomicBool>,
}
impl client::Handler for PinnedHost {
    type Error = russh::Error;
    async fn check_server_key(&mut self, key: &PublicKey) -> Result<bool, Self::Error> {
        let accepted = key.fingerprint(HashAlg::Sha256) == self.expected;
        self.rejected.store(!accepted, Ordering::Relaxed);
        Ok(accepted)
    }
}
fn private_key(key: &str, passphrase: Option<&str>) -> Result<PrivateKey, SshError> {
    if key.len() > 16_384 {
        return Err(SshError::PrivateKey);
    }
    let key = PrivateKey::from_openssh(key).map_err(|_| SshError::PrivateKey)?;
    match key.kdf() {
        Kdf::None => {}
        Kdf::Bcrypt { rounds, .. } if (1..=64).contains(rounds) => {}
        _ => return Err(SshError::KeyCost),
    }
    if key.is_encrypted() {
        key.decrypt(passphrase.ok_or(SshError::PrivateKey)?).map_err(|_| SshError::PrivateKey)
    } else if passphrase.is_some() {
        Err(SshError::PrivateKey)
    } else {
        Ok(key)
    }
}
fn output(bytes: Vec<u8>) -> SshOutput {
    match String::from_utf8(bytes) {
        Ok(text) => SshOutput::Utf8(text),
        Err(error) => {
            const HEX: &[u8; 16] = b"0123456789abcdef";
            let mut encoded = String::with_capacity(error.as_bytes().len() * 2);
            for byte in error.as_bytes() {
                encoded.push(HEX[(byte >> 4) as usize] as char);
                encoded.push(HEX[(byte & 15) as usize] as char);
            }
            SshOutput::Hex(encoded)
        }
    }
}
fn signal(signal: russh::Sig) -> String {
    use russh::Sig;
    match signal {
        Sig::ABRT => "ABRT",
        Sig::ALRM => "ALRM",
        Sig::FPE => "FPE",
        Sig::HUP => "HUP",
        Sig::ILL => "ILL",
        Sig::INT => "INT",
        Sig::KILL => "KILL",
        Sig::PIPE => "PIPE",
        Sig::QUIT => "QUIT",
        Sig::SEGV => "SEGV",
        Sig::TERM => "TERM",
        Sig::USR1 => "USR1",
        Sig::Custom(name) => return name,
    }
    .into()
}
pub(super) async fn execute(
    target: &SshTarget,
    authentication: SshAuthentication<'_>,
    action: SshAction<'_>,
    budget: SshBudget,
) -> Result<SshReport, SshError> {
    validate_target(target)?;
    if !(1..=600_000).contains(&budget.timeout_ms) || !(1..=16_777_216).contains(&budget.max_bytes)
    {
        return Err(SshError::InvalidBudget);
    }
    if let SshAction::Execute(command) = &action {
        if command.command.trim().is_empty() || command.command.len() > 65_536 {
            return Err(SshError::InvalidCommand);
        }
    }
    tokio::time::timeout(
        Duration::from_millis(budget.timeout_ms),
        run(target, authentication, action, budget),
    )
    .await
    .map_err(|_| SshError::Timeout)?
}
async fn run(
    target: &SshTarget,
    authentication: SshAuthentication<'_>,
    action: SshAction<'_>,
    budget: SshBudget,
) -> Result<SshReport, SshError> {
    let started = Instant::now();
    let rejected = Arc::new(AtomicBool::new(false));
    let config = Arc::new(client::Config {
        inactivity_timeout: Some(Duration::from_millis(budget.timeout_ms)),
        ..Default::default()
    });
    let stream =
        tokio::net::TcpStream::connect((target.host.as_str(), target.port)).await?.into_std()?;
    let _socket = OwnedSocket(stream.try_clone()?);
    let stream = tokio::net::TcpStream::from_std(stream)?;
    let mut handle = client::connect_stream(
        config,
        stream,
        PinnedHost { expected: fingerprint(target)?, rejected: rejected.clone() },
    )
    .await
    .map_err(|error| {
        if rejected.load(Ordering::Relaxed) {
            SshError::HostKeyMismatch
        } else {
            SshError::from(error)
        }
    })?;
    let authenticated = match authentication {
        SshAuthentication::Password(password) => {
            handle.authenticate_password(&target.username, password).await?
        }
        SshAuthentication::PrivateKey { key, passphrase } => {
            let key = Arc::new(private_key(key, passphrase)?);
            let hash = handle.best_supported_rsa_hash().await?.flatten();
            handle
                .authenticate_publickey(&target.username, PrivateKeyWithHashAlg::new(key, hash))
                .await?
        }
    };
    if !authenticated.success() {
        return Err(SshError::Authentication);
    }
    let report = match action {
        SshAction::Test => SshReport::Connected {
            host_key_sha256: target.host_key_sha256.clone(),
            elapsed_ms: started.elapsed().as_millis() as u64,
        },
        SshAction::Execute(command) => {
            let mut channel = handle.channel_open_session().await?;
            channel.exec(true, command.command.as_bytes()).await?;
            let mut stdout = Vec::new();
            let mut stderr = Vec::new();
            let mut termination = None;
            while let Some(message) = channel.wait().await {
                match message {
                    ChannelMsg::Data { data } => {
                        if stdout.len() + stderr.len() + data.len() > budget.max_bytes {
                            return Err(SshError::OutputLimit);
                        }
                        stdout.extend_from_slice(&data);
                    }
                    ChannelMsg::ExtendedData { data, ext: 1 } => {
                        if stdout.len() + stderr.len() + data.len() > budget.max_bytes {
                            return Err(SshError::OutputLimit);
                        }
                        stderr.extend_from_slice(&data);
                    }
                    ChannelMsg::ExtendedData { .. } => return Err(SshError::ExtendedStream),
                    ChannelMsg::ExitStatus { exit_status } => {
                        if termination.replace(SshTermination::Exit { code: exit_status }).is_some()
                        {
                            return Err(SshError::Termination);
                        }
                    }
                    ChannelMsg::ExitSignal { signal_name, core_dumped, error_message, .. } => {
                        if error_message.len() > budget.max_bytes {
                            return Err(SshError::OutputLimit);
                        }
                        if termination
                            .replace(SshTermination::Signal {
                                name: signal(signal_name),
                                core_dumped,
                                message: error_message,
                            })
                            .is_some()
                        {
                            return Err(SshError::Termination);
                        }
                    }
                    ChannelMsg::Failure => return Err(SshError::ExecRejected),
                    ChannelMsg::Close => break,
                    _ => {}
                }
            }
            SshReport::Executed {
                host_key_sha256: target.host_key_sha256.clone(),
                stdout: output(stdout),
                stderr: output(stderr),
                termination: termination.unwrap_or(SshTermination::Missing),
                elapsed_ms: started.elapsed().as_millis() as u64,
            }
        }
    };
    let _ = handle.disconnect(russh::Disconnect::ByApplication, "operation complete", "").await;
    if serde_json::to_vec(&report).map_err(|_| SshError::OutputLimit)?.len() > budget.max_bytes {
        return Err(SshError::OutputLimit);
    }
    Ok(report)
}
