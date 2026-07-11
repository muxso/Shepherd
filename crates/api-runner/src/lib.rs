//! 接口用例执行内核:RequestSpec 请求描述、断言求值(MatchCondition/Assertion)、`${var}` 变量替换、
//! 提取器(Extractor)与前后置处理(Processor),产出 CaseReport。
//! domain 只做纯计算不发请求;ReqwestRunner 适配器负责真实 HTTP 执行,供场景/批量/压测/runner-agent 复用。

pub mod adapters;
pub mod domain;

pub use adapters::ReqwestRunner;
pub use domain::{
    env_extracts, evaluate, evaluate_detailed, evaluate_detailed_with_vars, evaluate_with_vars,
    run_extracts, substitute, wait_millis, Assertion, AssertionReport, CaseOutcome, CaseReport,
    ExtractKind, ExtractScope, Extractor, HttpMethod, MatchCondition, Processor, RequestSpec,
    ResponseSnapshot,
};
