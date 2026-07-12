//! Remote executor context: RunnerAgent registry (name/base_url/supported
//! protocols/enable-disable). The RemoteRunner/RemoteProbe ports forward API
//! cases and probe requests to a remote runner-agent for execution,
//! ExecutionStore records results, and DispatchTarget picks local vs. a
//! specific agent. domain/application/ports do no IO; pg/http/client (reqwest
//! remote calls) adapters are feature-gated.

pub mod adapters;
pub mod application;
pub mod domain;
pub mod ports;
