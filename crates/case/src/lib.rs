//! Case review context: ReviewRecord and its state machine (unreviewed / in review /
//! passed / failed / re-review); PassRule decides single- vs multi-reviewer pass criteria;
//! ReviewSetting is the per-project review config.
//! domain/application/ports do no IO; pg/http adapters are feature-gated.

pub mod adapters;
pub mod application;
pub mod domain;
pub mod ports;
