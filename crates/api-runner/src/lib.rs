//! API case execution kernel: RequestSpec request descriptions, assertion evaluation
//! (MatchCondition/Assertion), `${var}` substitution, extractors and pre/post
//! processors, producing a CaseReport.
//!
//! The domain layer is pure computation and sends no requests; the ReqwestRunner
//! adapter does real HTTP, reused by scenario/batch/load-test/runner-agent.

pub mod adapters;
pub mod domain;

pub use adapters::ReqwestRunner;
pub use domain::{
    env_extracts, evaluate, evaluate_detailed, evaluate_detailed_with_vars, evaluate_with_vars,
    run_extracts, substitute, wait_millis, Assertion, AssertionReport, CaseOutcome, CaseReport,
    ExtractKind, ExtractScope, Extractor, HttpMethod, MatchCondition, PhaseTimings, Processor,
    RequestSpec, ResponseSnapshot,
};
