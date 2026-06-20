//! 领域层:请求规格、响应快照、断言模型 + 纯函数判定。
pub mod runner;

pub use runner::{
    evaluate, evaluate_detailed, run_extracts, substitute, wait_millis, AssertionReport, Assertion,
    CaseOutcome, CaseReport, ExtractKind, Extractor, HttpMethod, MatchCondition, Processor,
    RequestSpec, ResponseSnapshot,
};
