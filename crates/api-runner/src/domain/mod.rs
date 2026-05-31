//! 领域层:请求规格、响应快照、断言模型 + 纯函数判定。
pub mod runner;

pub use runner::{
    evaluate, Assertion, CaseOutcome, CaseReport, HttpMethod, RequestSpec, ResponseSnapshot,
};
