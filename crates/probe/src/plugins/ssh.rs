use async_trait::async_trait;
use std::sync::Arc;
use std::time::Instant;

use russh::client::{self, Handler};
use russh::keys::PublicKey;
use russh::ChannelMsg;

use crate::domain::{ProbeRequest, RawProbe};
use crate::ports::ProtocolPlugin;

#[derive(Default)]
pub struct SshPlugin;

impl SshPlugin {
    pub fn new() -> Self {
        Self
    }
}

// Debug/probe use case: skip host key verification (accept any), matching the one-shot connection semantics.
struct AcceptAll;

impl Handler for AcceptAll {
    type Error = russh::Error;
    async fn check_server_key(&mut self, _key: &PublicKey) -> Result<bool, Self::Error> {
        Ok(true)
    }
}

async fn run_ssh(
    target: &str,
    command: &str,
    user: &str,
    password: &str,
) -> Result<(String, i64), String> {
    let (host, port) = match target.rsplit_once(':') {
        Some((h, p)) => (h.to_string(), p.parse::<u16>().map_err(|_| "invalid port".to_string())?),
        None => (target.to_string(), 22u16),
    };
    let config = Arc::new(client::Config::default());
    let mut handle = client::connect(config, (host.as_str(), port), AcceptAll)
        .await
        .map_err(|e| format!("connection failed: {e}"))?;
    let authed = handle
        .authenticate_password(user, password)
        .await
        .map_err(|e| format!("authentication error: {e}"))?;
    if !authed.success() {
        return Err("authentication failed (username/password)".to_string());
    }
    let mut channel =
        handle.channel_open_session().await.map_err(|e| format!("failed to open channel: {e}"))?;
    channel.exec(true, command).await.map_err(|e| format!("execution failed: {e}"))?;

    let mut out: Vec<u8> = Vec::new();
    let mut code: i64 = 0;
    while let Some(msg) = channel.wait().await {
        match msg {
            ChannelMsg::Data { ref data } => out.extend_from_slice(data),
            ChannelMsg::ExtendedData { ref data, .. } => out.extend_from_slice(data),
            ChannelMsg::ExitStatus { exit_status } => code = exit_status as i64,
            _ => {}
        }
    }
    Ok((String::from_utf8_lossy(&out).into_owned(), code))
}

#[async_trait]
impl ProtocolPlugin for SshPlugin {
    fn protocol(&self) -> &'static str {
        "ssh"
    }

    async fn run(&self, req: &ProbeRequest) -> RawProbe {
        let command = req.payload.clone().unwrap_or_else(|| "echo ok".to_string());
        let user = req.metadata.get("user").cloned().unwrap_or_default();
        let password = req.metadata.get("password").cloned().unwrap_or_default();
        let t = Instant::now();
        match run_ssh(&req.target, &command, &user, &password).await {
            Ok((output, code)) => RawProbe {
                transport_ok: true,
                status: Some(code),
                latency_ms: t.elapsed().as_millis() as u64,
                output: Some(output),
                error: None,
            },
            Err(e) => RawProbe {
                transport_ok: false,
                latency_ms: t.elapsed().as_millis() as u64,
                error: Some(e),
                ..Default::default()
            },
        }
    }
}
