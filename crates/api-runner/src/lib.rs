//! api-runner —— 原生 Rust 接口测试执行器(替代 JMeter)
//!
//! 设计同其余模块:**断言判定是纯函数(domain,零 IO,TDD 全覆盖)**,
//! HTTP 执行是 adapter(reqwest)。一个接口用例 = 一个 `RequestSpec` + 一组 `Assertion`,
//! 执行得到 `ResponseSnapshot` 后由 `evaluate` 纯函数判定通过/失败。
//!
//! 这就是「不用 JMeter」的答案:功能接口测试用它即可;若需分布式压测,
//! 同一 `RequestSpec` 模型可喂给基于 tokio 的负载框架(如 Goose)。

pub mod adapters;
pub mod domain;

pub use adapters::ReqwestRunner;
pub use domain::{
    env_extracts, evaluate, evaluate_detailed, evaluate_detailed_with_vars, evaluate_with_vars,
    run_extracts, substitute, wait_millis, AssertionReport, Assertion, CaseOutcome, CaseReport,
    ExtractKind, ExtractScope, Extractor, HttpMethod, MatchCondition, Processor, RequestSpec,
    ResponseSnapshot,
};
