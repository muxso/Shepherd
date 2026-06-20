//! 把压测引擎接到原生 reqwest 执行器(复用 `api-runner`)—— 这就是"不用 JMeter"的压测后端。
//!
//! 预置被测请求 + 断言;每次 `execute` 跑一次 `run_case`,成功 = 断言全过(含 HTTP 传输成功)。
//! 多个 worker 共享同一个 `ReqwestRunner`(内部 reqwest client 连接池可复用)。

use async_trait::async_trait;

use api_runner::{Assertion, CaseOutcome, ReqwestRunner, RequestSpec};

use crate::ports::RequestExecutor;

pub struct ApiRunnerExecutor {
    runner: ReqwestRunner,
    spec: RequestSpec,
    assertions: Vec<Assertion>,
}

impl ApiRunnerExecutor {
    /// 用给定请求 + 断言构造(默认 `no_proxy` 直连被测主机,与本仓其它执行器一致)。
    pub fn new(spec: RequestSpec, assertions: Vec<Assertion>) -> Self {
        Self { runner: ReqwestRunner::no_proxy(), spec, assertions }
    }

    /// 注入自定义 runner(自定超时/连接池/代理)。
    pub fn with_runner(runner: ReqwestRunner, spec: RequestSpec, assertions: Vec<Assertion>) -> Self {
        Self { runner, spec, assertions }
    }
}

#[async_trait]
impl RequestExecutor for ApiRunnerExecutor {
    async fn execute(&self) -> bool {
        self.runner.run_case(&self.spec, &self.assertions).await.outcome == CaseOutcome::Success
    }
}
