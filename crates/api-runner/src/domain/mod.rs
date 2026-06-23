//! 领域层:请求规格、响应快照、断言模型 + 纯函数判定。
pub mod runner;

pub use runner::{
    env_extracts, evaluate, evaluate_detailed, evaluate_detailed_with_vars, evaluate_with_vars,
    run_extracts, substitute, wait_millis, AssertionReport, Assertion, CaseOutcome, CaseReport,
    ExtractKind, ExtractScope, Extractor, HttpMethod, MatchCondition, Processor, RequestSpec,
    ResponseSnapshot,
};
