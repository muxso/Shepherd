#![allow(dead_code)]
//! Loopback SSH fixture with throwaway OpenSSH keys and observable connection cleanup.
use probe::domain::ssh::SshTarget;
use russh::{
    keys::{HashAlg, PrivateKey, PublicKey},
    server::{self, Server as _},
    Channel, ChannelId,
};
use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
#[derive(Clone)]
struct Shared {
    keys: Vec<PublicKey>,
    active: Arc<AtomicUsize>,
    attempts: Arc<AtomicUsize>,
    commands: Arc<Mutex<Vec<String>>>,
}
struct Factory(Shared);
struct Handler(Shared);
impl server::Server for Factory {
    type Handler = Handler;
    fn new_client(&mut self, _: Option<std::net::SocketAddr>) -> Handler {
        self.0.active.fetch_add(1, Ordering::SeqCst);
        Handler(self.0.clone())
    }
}
impl Drop for Handler {
    fn drop(&mut self) {
        self.0.active.fetch_sub(1, Ordering::SeqCst);
    }
}
impl server::Handler for Handler {
    type Error = russh::Error;
    async fn auth_password(
        &mut self,
        user: &str,
        password: &str,
    ) -> Result<server::Auth, Self::Error> {
        self.0.attempts.fetch_add(1, Ordering::SeqCst);
        Ok(if user == "fixture" && password == "fixture password" {
            server::Auth::Accept
        } else {
            server::Auth::reject()
        })
    }
    async fn auth_publickey(
        &mut self,
        user: &str,
        key: &PublicKey,
    ) -> Result<server::Auth, Self::Error> {
        self.0.attempts.fetch_add(1, Ordering::SeqCst);
        Ok(
            if user == "fixture"
                && self.0.keys.iter().any(|allowed| allowed.key_data() == key.key_data())
            {
                server::Auth::Accept
            } else {
                server::Auth::reject()
            },
        )
    }
    async fn channel_open_session(
        &mut self,
        _: Channel<server::Msg>,
        reply: server::ChannelOpenHandle,
        _: &mut server::Session,
    ) -> Result<(), Self::Error> {
        reply.accept().await;
        Ok(())
    }
    async fn exec_request(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut server::Session,
    ) -> Result<(), Self::Error> {
        let command = String::from_utf8(data.to_vec()).expect("UTF8 request");
        self.0.commands.lock().expect("commands").push(command.clone());
        if command == "reject" {
            session.channel_failure(channel)?;
            session.close(channel)?;
            return Ok(());
        }
        session.channel_success(channel)?;
        match command.as_str() {
            "block" => return Ok(()),
            "streams" => {
                session.data(channel, b"out with spaces\n".to_vec())?;
                session.extended_data(channel, 1, b"error output\n".to_vec())?;
                session.exit_status_request(channel, 7)?;
            }
            "missing-status" => {}
            "signal" => {
                session.exit_signal_request(channel, russh::Sig::TERM, false, "terminated", "")?;
            }
            "large" => {
                session.data(channel, vec![b'x'; 4096])?;
                session.exit_status_request(channel, 0)?;
            }
            "binary" => {
                session.data(channel, vec![255, 0])?;
                session.exit_status_request(channel, 0)?;
            }
            command => {
                session.data(
                    channel,
                    command.strip_prefix("echo ").unwrap_or("ok").as_bytes().to_vec(),
                )?;
                session.exit_status_request(channel, 0)?;
            }
        }
        session.eof(channel)?;
        session.close(channel)?;
        Ok(())
    }
}
pub struct Fixture {
    pub target: SshTarget,
    pub private_key: String,
    pub encrypted_key: String,
    pub wrong_fingerprint: String,
    shared: Shared,
    shutdown: server::RunningServerHandle,
    task: tokio::task::JoinHandle<std::io::Result<()>>,
    _directory: tempfile::TempDir,
}
fn generate(path: &std::path::Path, passphrase: &str) -> String {
    let result = std::process::Command::new("ssh-keygen")
        .args(["-q", "-t", "ed25519", "-a", "4", "-N", passphrase, "-f"])
        .arg(path)
        .output()
        .expect("ssh-keygen must be installed for the fixture");
    assert!(result.status.success(), "fixture key generation failed");
    std::fs::read_to_string(path).expect("fixture key")
}
impl Fixture {
    pub async fn start() -> Self {
        let directory = tempfile::tempdir().expect("key directory");
        let host = generate(&directory.path().join("host"), "");
        let private_key = generate(&directory.path().join("client"), "");
        let encrypted_key = generate(&directory.path().join("encrypted"), "fixture passphrase");
        let host = PrivateKey::from_openssh(host).expect("host key");
        let client = PrivateKey::from_openssh(&private_key).expect("client key");
        let encrypted = PrivateKey::from_openssh(&encrypted_key).expect("encrypted key");
        let target = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("SSH listener");
        let port = target.local_addr().expect("address").port();
        let fingerprint = host.public_key().fingerprint(HashAlg::Sha256).to_string();
        let wrong_fingerprint = client.public_key().fingerprint(HashAlg::Sha256).to_string();
        let shared = Shared {
            keys: vec![client.public_key().clone(), encrypted.public_key().clone()],
            active: Arc::new(AtomicUsize::new(0)),
            attempts: Arc::new(AtomicUsize::new(0)),
            commands: Arc::new(Mutex::new(Vec::new())),
        };
        let config = Arc::new(server::Config {
            keys: vec![host],
            auth_rejection_time: Duration::ZERO,
            auth_rejection_time_initial: Some(Duration::ZERO),
            ..Default::default()
        });
        let state = shared.clone();
        let (send, receive) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            let mut server = Factory(state);
            let running = server.run_on_socket(config, &target);
            let _ = send.send(running.handle());
            running.await
        });
        let shutdown = receive.await.expect("SSH server startup");
        Self {
            target: SshTarget {
                host: "127.0.0.1".into(),
                port,
                username: "fixture".into(),
                host_key_sha256: fingerprint,
            },
            private_key,
            encrypted_key,
            wrong_fingerprint,
            shared,
            shutdown,
            task,
            _directory: directory,
        }
    }
    pub fn attempts(&self) -> usize {
        self.shared.attempts.load(Ordering::SeqCst)
    }
    pub async fn idle(&self) {
        for _ in 0..100 {
            if self.shared.active.load(Ordering::SeqCst) == 0 {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("owned SSH connection did not close");
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.shutdown.shutdown("fixture complete".into());
        self.task.abort();
    }
}
