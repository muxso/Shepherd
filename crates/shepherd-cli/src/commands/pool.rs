use crate::client::*;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum PoolCmd {
    /// Create a resource pool.
    Create {
        #[arg(long)]
        name: String,
        /// Mark disabled (enabled by default).
        #[arg(long, default_value_t = false)]
        disable: bool,
    },
    /// List resource pools.
    List,
    /// Online runner counts per pool.
    Status,
    /// Per-pool connected runner details (name / capacity / in-flight).
    StatusDetail,
}

pub fn run(cmd: PoolCmd) -> R<()> {
    let c = Client::new(Config::load())?;
    match cmd {
        PoolCmd::Create { name, disable } => pretty(&c.post(
            "/api/resource-pool",
            json!({"name": name, "enabled": !disable}),
            true,
        )?),
        PoolCmd::List => pretty(&c.get("/api/resource-pool", true)?),
        PoolCmd::Status => pretty(&c.get("/api/pool-runner/status", true)?),
        PoolCmd::StatusDetail => pretty(&c.get("/api/pool-runner/status/detail", true)?),
    };
    Ok(())
}
