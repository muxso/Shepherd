//! 用例:发起批量运行。
//!
//! 编排顺序(把 quirk 前移为入口校验):
//!  1. 校验命令(用例非空、重试配置合法)
//!  2. 解析有效池:客户端 poolId 优先,否则回退项目默认池;皆空 → ResourcePoolNotConfigured
//!  3. 校验池可用;不可用 → ResourcePoolUnavailable
//!  4. 派发执行,返回报告 id
//!
//! 对比 Java 版:第 2/3 步的失败原本要等任务下发后深在执行链里 500;这里在入口就明确失败。

use std::sync::Arc;

use crate::domain::{resolve_effective_pool, BatchRunCommand, BatchRunError, ResolvedEnv, RunModeConfig};
use crate::ports::{
    BatchExecutorPort, DispatchReport, DispatchSpec, EnvironmentPort, PortError, ResourcePoolPort,
};

impl From<PortError> for BatchRunError {
    fn from(e: PortError) -> Self {
        match e {
            PortError::Backend(m) => BatchRunError::Backend(m),
        }
    }
}

#[derive(Clone)]
pub struct StartBatchRunUseCase {
    pools: Arc<dyn ResourcePoolPort>,
    executor: Arc<dyn BatchExecutorPort>,
    envs: Arc<dyn EnvironmentPort>,
}

impl StartBatchRunUseCase {
    pub fn new(
        pools: Arc<dyn ResourcePoolPort>,
        executor: Arc<dyn BatchExecutorPort>,
        envs: Arc<dyn EnvironmentPort>,
    ) -> Self {
        Self { pools, executor, envs }
    }

    /// 返回派发后的报告 id。
    pub async fn execute(
        &self,
        project_id: &str,
        case_ids: Vec<String>,
        config: RunModeConfig,
    ) -> Result<DispatchReport, BatchRunError> {
        // 1) 命令校验
        let cmd = BatchRunCommand::new(case_ids, config)?;

        // 2) 解析有效池(客户端优先,回退项目默认)
        let default_pool = self.pools.default_pool_id(project_id).await?;
        let pool_id =
            resolve_effective_pool(cmd.config.pool_id.as_deref(), default_pool.as_deref())?;

        // 3) 池必须可用
        if !self.pools.is_pool_available(&pool_id).await? {
            return Err(BatchRunError::ResourcePoolUnavailable { pool_id });
        }

        // 4) 解析运行环境(给定 environmentId 才解析;缺失/查无 → 空环境,不注入)
        let environment_id =
            cmd.config.environment_id.as_deref().filter(|s| !s.trim().is_empty()).map(str::to_string);
        let env = match environment_id.as_deref() {
            Some(id) => self.envs.resolve(id).await?.unwrap_or_default(),
            None => ResolvedEnv::default(),
        };

        // 5) 派发
        let spec =
            DispatchSpec { case_ids: cmd.case_ids, pool_id, mode: cmd.config.mode, env, environment_id };
        Ok(self.executor.dispatch(&spec).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::{FakeEnvironment, FakeResourcePool, SpyExecutor};
    use crate::domain::BatchRunMode;

    fn config(pool: Option<&str>) -> RunModeConfig {
        RunModeConfig {
            mode: BatchRunMode::Parallel,
            pool_id: pool.map(str::to_string),
            retry: None,
            environment_id: None,
        }
    }

    fn uc(pools: FakeResourcePool, exec: SpyExecutor) -> StartBatchRunUseCase {
        StartBatchRunUseCase::new(Arc::new(pools), Arc::new(exec), Arc::new(FakeEnvironment::new()))
    }

    #[tokio::test]
    async fn no_pool_anywhere_is_explicit_error_not_dispatched() {
        // 项目无默认池 + 客户端未传 → 入口明确失败(对应 Java 的 500 场景)
        let exec = SpyExecutor::new();
        let uc = uc(FakeResourcePool::new(), exec.clone());
        let err = uc.execute("proj1", vec!["c1".into()], config(None)).await.unwrap_err();
        assert_eq!(err, BatchRunError::ResourcePoolNotConfigured);
        assert_eq!(exec.dispatch_count(), 0); // 没有派发
    }

    #[tokio::test]
    async fn falls_back_to_project_default_pool() {
        let exec = SpyExecutor::new();
        let pools = FakeResourcePool::new().with_default("proj1", "proj-pool").with_available("proj-pool");
        let uc = uc(pools, exec.clone());
        uc.execute("proj1", vec!["c1".into()], config(None)).await.expect("ok");
        assert_eq!(exec.last_pool(), Some("proj-pool".to_string()));
    }

    #[tokio::test]
    async fn client_pool_wins_over_project_default() {
        let exec = SpyExecutor::new();
        let pools = FakeResourcePool::new()
            .with_default("proj1", "proj-pool")
            .with_available("proj-pool")
            .with_available("client-pool");
        let uc = uc(pools, exec.clone());
        uc.execute("proj1", vec!["c1".into(), "c2".into()], config(Some("client-pool")))
            .await
            .expect("ok");
        assert_eq!(exec.last_pool(), Some("client-pool".to_string()));
        assert_eq!(exec.last_case_count(), Some(2)); // 全部用例透传
    }

    #[tokio::test]
    async fn resolved_pool_unavailable_is_rejected() {
        let exec = SpyExecutor::new();
        // 池已解析(客户端传了)但不在可用集合里
        let uc = uc(FakeResourcePool::new(), exec.clone());
        let err = uc.execute("proj1", vec!["c1".into()], config(Some("dead-pool"))).await.unwrap_err();
        assert_eq!(err, BatchRunError::ResourcePoolUnavailable { pool_id: "dead-pool".into() });
        assert_eq!(exec.dispatch_count(), 0);
    }

    #[tokio::test]
    async fn resolves_environment_into_dispatch_spec() {
        use crate::adapters::FakeEnvironment;
        let exec = SpyExecutor::new();
        let pools = FakeResourcePool::new().with_available("client-pool");
        let env = ResolvedEnv {
            base_url: "http://h".into(),
            headers: vec![("A".into(), "b".into())],
            variables: Default::default(),
        };
        let envs = FakeEnvironment::new().with("e1", env.clone());
        let uc = StartBatchRunUseCase::new(Arc::new(pools), Arc::new(exec.clone()), Arc::new(envs));
        let mut cfg = config(Some("client-pool"));
        cfg.environment_id = Some("e1".into());
        uc.execute("proj1", vec!["c1".into()], cfg).await.expect("ok");
        assert_eq!(exec.last_env(), Some(env)); // 环境解析并透传到派发规格
    }

    #[tokio::test]
    async fn empty_cases_rejected_before_touching_pools() {
        let exec = SpyExecutor::new();
        let uc = uc(FakeResourcePool::new(), exec.clone());
        let err = uc.execute("proj1", vec![], config(Some("client-pool"))).await.unwrap_err();
        assert_eq!(err, BatchRunError::NoCases);
        assert_eq!(exec.dispatch_count(), 0);
    }

    #[tokio::test]
    async fn dispatch_returns_report_id() {
        let exec = SpyExecutor::new();
        let pools = FakeResourcePool::new().with_available("client-pool");
        let uc = uc(pools, exec.clone());
        let report = uc
            .execute("proj1", vec!["c1".into()], config(Some("client-pool")))
            .await
            .expect("ok");
        assert!(report.report_id.starts_with("report-"));
        assert_eq!(report.status, "SUCCESS");
    }
}
