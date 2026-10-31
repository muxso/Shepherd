use super::ssh_command::{SshBudget, SshError};
use crate::{
    domain::{
        ssh::{
            SshAction, SshAuthentication, SshCommand, SshOutput, SshReport, SshTarget,
            SshTermination,
        },
        ProbeRequest, RawProbe,
    },
    ports::ProtocolPlugin,
};
use async_trait::async_trait;
use std::{
    net::{IpAddr, SocketAddr},
    time::Instant,
};
#[derive(Default)]
pub struct SshPlugin;
impl SshPlugin {
    pub fn new() -> Self {
        Self
    }
    /// Profiles and legacy probes share the same pinned, bounded transport.
    pub async fn execute_command(
        &self,
        target: &SshTarget,
        authentication: SshAuthentication<'_>,
        action: SshAction<'_>,
        budget: SshBudget,
    ) -> Result<SshReport, SshError> {
        super::ssh_command::execute(target, authentication, action, budget).await
    }
}
fn target(request: &ProbeRequest) -> Result<SshTarget, SshError> {
    let (host, port) = if let Ok(address) = request.target.parse::<SocketAddr>() {
        (address.ip().to_string(), address.port())
    } else if let Ok(address) =
        request.target.trim_start_matches('[').trim_end_matches(']').parse::<IpAddr>()
    {
        (address.to_string(), 22)
    } else if let Some((host, port)) = request.target.rsplit_once(':') {
        if host.contains(':') {
            return Err(SshError::InvalidTarget);
        }
        (host.into(), port.parse().map_err(|_| SshError::InvalidTarget)?)
    } else {
        (request.target.clone(), 22)
    };
    Ok(SshTarget {
        host,
        port,
        username: request.metadata.get("user").cloned().unwrap_or_default(),
        host_key_sha256: request.metadata.get("host_key_sha256").cloned().unwrap_or_default(),
    })
}
fn output(output: SshOutput) -> String {
    match output {
        SshOutput::Utf8(text) => text,
        SshOutput::Hex(hex) => format!("hex:{hex}"),
    }
}
#[async_trait]
impl ProtocolPlugin for SshPlugin {
    fn protocol(&self) -> &'static str {
        "ssh"
    }
    async fn run(&self, request: &ProbeRequest) -> RawProbe {
        let started = Instant::now();
        let result = async {
            let target = target(request)?;
            let timeout_ms = request
                .metadata
                .get("timeout_ms")
                .map(|value| value.parse::<u64>().map_err(|_| SshError::InvalidBudget))
                .transpose()?
                .unwrap_or(30_000);
            let password = request.metadata.get("password").map(String::as_str).unwrap_or("");
            let command =
                SshCommand { command: request.payload.clone().unwrap_or_else(|| "echo ok".into()) };
            self.execute_command(
                &target,
                SshAuthentication::Password(password),
                SshAction::Execute(&command),
                SshBudget { timeout_ms, max_bytes: 1_048_576 },
            )
            .await
        }
        .await;
        match result {
            Ok(SshReport::Executed { stdout, stderr, termination, .. }) => {
                let status = match termination {
                    SshTermination::Exit { code } => Some(i64::from(code)),
                    _ => None,
                };
                RawProbe {
                    transport_ok: status.is_some(),
                    status,
                    latency_ms: started.elapsed().as_millis() as u64,
                    output: Some(output(stdout) + &output(stderr)),
                    error: if status.is_none() {
                        Some("SSH command did not report an exit code".into())
                    } else {
                        None
                    },
                }
            }
            Ok(_) => RawProbe {
                transport_ok: false,
                error: Some("unexpected SSH connection report".into()),
                ..Default::default()
            },
            Err(error) => RawProbe {
                transport_ok: false,
                latency_ms: started.elapsed().as_millis() as u64,
                error: Some(error.to_string()),
                ..Default::default()
            },
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn unverified_legacy_connections_are_rejected_before_network_access() {
        let request = ProbeRequest {
            protocol: "ssh".into(),
            target: "127.0.0.1:1".into(),
            payload: Some("echo ok".into()),
            metadata: [("user".into(), "fixture".into()), ("password".into(), "fixture".into())]
                .into(),
            assertions: vec![],
        };
        let result = SshPlugin::new().run(&request).await;
        assert!(!result.transport_ok);
        assert!(result.error.expect("error").contains("SHA256"));
    }
}
