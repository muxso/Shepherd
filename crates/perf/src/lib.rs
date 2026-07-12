//! Load-testing context: LoadPlan defines the load (iteration count or duration),
//! the engine drives a RequestExecutor accordingly (api-runner-exec/probe-exec
//! reuse api-runner and probe respectively). Samples flow through a SampleSink
//! (in-memory or parquet-sink) and aggregate into LatencyStats/LoadReport.
//! domain/ports do no IO; executors and sink adapters are feature-gated.

pub mod adapters;
pub mod domain;
pub mod ports;
