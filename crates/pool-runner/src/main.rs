//! Pool runner daemon: dials the Shepherd server over WebSocket (runners sit
//! behind NAT, so all traffic is outbound), registers under a resource pool and
//! executes dispatched scenario plans locally.
//!
//! Configuration (env):
//!   SHEPHERD_SERVER_WS  ws(s)://host:port/api/pool-runner/ws (required)
//!   SHEPHERD_POOL_ID    resource pool id to register under (required)
//!   SHEPHERD_RUNNER_KEY API key (sak_...) or session token (required)
//!   RUNNER_NAME         display name (default: hostname)
//!   RUNNER_MAX_CONCURRENT concurrent run cap advertised to the server (default 4)

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;

use pool_runner::execute_assignment;
use pool_runner::protocol::{RunnerMsg, ServerMsg};

struct Config {
    ws_url: String,
    pool_id: String,
    key: String,
    name: String,
    max_concurrent: u32,
}

impl Config {
    fn from_env() -> Result<Self, String> {
        let need = |k: &str| std::env::var(k).map_err(|_| format!("missing env {k}"));
        Ok(Self {
            ws_url: need("SHEPHERD_SERVER_WS")?,
            pool_id: need("SHEPHERD_POOL_ID")?,
            key: need("SHEPHERD_RUNNER_KEY")?,
            name: std::env::var("RUNNER_NAME").unwrap_or_else(|_| {
                std::process::Command::new("hostname")
                    .output()
                    .ok()
                    .and_then(|o| String::from_utf8(o.stdout).ok())
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| "pool-runner".to_string())
            }),
            max_concurrent: std::env::var("RUNNER_MAX_CONCURRENT")
                .ok()
                .and_then(|s| s.parse().ok())
                .filter(|&n| n >= 1)
                .unwrap_or_else(pool_runner::protocol::default_max_concurrent),
        })
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();
    let cfg = match Config::from_env() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("pool-runner: {e}");
            std::process::exit(2);
        }
    };
    tracing::info!(pool = %cfg.pool_id, name = %cfg.name, server = %cfg.ws_url, max_concurrent = cfg.max_concurrent, "pool-runner starting");
    let mut backoff = 1u64;
    loop {
        match serve_once(&cfg).await {
            Ok(()) => {
                tracing::warn!("connection closed; reconnecting");
                backoff = 1;
            }
            Err(e) => {
                tracing::warn!(error = %e, "connect failed; retry in {backoff}s");
            }
        }
        tokio::time::sleep(Duration::from_secs(backoff)).await;
        backoff = (backoff * 2).min(30);
    }
}

/// One connection lifetime: hello, then serve dispatches until the socket dies.
async fn serve_once(cfg: &Config) -> Result<(), String> {
    let mut req = cfg.ws_url.as_str().into_client_request().map_err(|e| e.to_string())?;
    let auth = format!("Bearer {}", cfg.key);
    req.headers_mut()
        .insert("authorization", auth.parse().map_err(|_| "bad key format".to_string())?);
    let (ws, _) = connect_async(req).await.map_err(|e| e.to_string())?;
    tracing::info!("connected");
    let (mut write, mut read) = ws.split();

    // Single writer: run tasks push RunnerMsg (or raw frames) through this channel.
    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();
    let writer = tokio::spawn(async move {
        while let Some(m) = rx.recv().await {
            if write.send(m).await.is_err() {
                break;
            }
        }
    });

    let send_msg = |tx: &mpsc::UnboundedSender<Message>, m: &RunnerMsg| {
        if let Ok(s) = serde_json::to_string(m) {
            let _ = tx.send(Message::Text(s));
        }
    };
    send_msg(
        &tx,
        &RunnerMsg::Hello {
            pool_id: cfg.pool_id.clone(),
            name: cfg.name.clone(),
            capabilities: vec!["http".to_string()],
            max_concurrent: cfg.max_concurrent,
        },
    );

    let permits = std::sync::Arc::new(tokio::sync::Semaphore::new(cfg.max_concurrent as usize));

    // Event forwarder: RunnerMsg from executing runs → text frames.
    let (ev_tx, mut ev_rx) = mpsc::unbounded_channel::<RunnerMsg>();
    let tx_events = tx.clone();
    tokio::spawn(async move {
        while let Some(m) = ev_rx.recv().await {
            if let Ok(s) = serde_json::to_string(&m) {
                if tx_events.send(Message::Text(s)).is_err() {
                    break;
                }
            }
        }
    });

    while let Some(frame) = read.next().await {
        let frame = frame.map_err(|e| e.to_string())?;
        match frame {
            Message::Text(text) => match serde_json::from_str::<ServerMsg>(&text) {
                Ok(ServerMsg::HelloAck { runner_id }) => {
                    tracing::info!(%runner_id, "registered");
                }
                Ok(ServerMsg::Run { run_id, nodes, env, stop_on_failure }) => {
                    tracing::info!(%run_id, steps = nodes.len(), "run dispatched");
                    let ev = ev_tx.clone();
                    let permits = permits.clone();
                    tokio::spawn(async move {
                        // Local guard mirroring the advertised cap; the server
                        // already schedules within it.
                        let _permit = permits.acquire_owned().await;
                        let id = run_id.clone();
                        execute_assignment(run_id, nodes, env, stop_on_failure, ev).await;
                        tracing::info!(run_id = %id, "run finished");
                    });
                }
                Err(e) => tracing::debug!(error = %e, "unrecognized server frame"),
            },
            Message::Ping(payload) => {
                let _ = tx.send(Message::Pong(payload));
            }
            Message::Close(_) => break,
            _ => {}
        }
    }
    writer.abort();
    Ok(())
}
