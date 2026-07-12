use async_trait::async_trait;

use api_runner::{Assertion, CaseOutcome, RequestSpec, ReqwestRunner};

use crate::ports::RequestExecutor;

pub struct ApiRunnerExecutor {
    runner: ReqwestRunner,
    spec: RequestSpec,
    assertions: Vec<Assertion>,
}

impl ApiRunnerExecutor {
    pub fn new(spec: RequestSpec, assertions: Vec<Assertion>) -> Self {
        Self { runner: ReqwestRunner::no_proxy(), spec, assertions }
    }

    pub fn with_runner(
        runner: ReqwestRunner,
        spec: RequestSpec,
        assertions: Vec<Assertion>,
    ) -> Self {
        Self { runner, spec, assertions }
    }
}

#[async_trait]
impl RequestExecutor for ApiRunnerExecutor {
    async fn execute(&self) -> bool {
        self.runner.run_case(&self.spec, &self.assertions).await.outcome == CaseOutcome::Success
    }
}
