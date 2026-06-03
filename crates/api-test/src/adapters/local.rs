//! 原生本地执行器:实现 `crate::ports::TaskDispatcher`,用 `api-runner`
//! 就地跑用例,**完全不依赖 JMeter**。
//!
//! 能力:
//!  - 取每个 case 的请求规格+断言(`CaseSpecSource`)→ reqwest 执行+判定;
//!  - **per-case 结果明细**写出(`CaseResultSink`:成功/失败 + 失败原因列表);
//!  - **并发执行**:PARALLEL 模式按并发上限并行跑,SERIAL 顺序跑;
//!  - 聚合为整体状态(任一失败/缺失即 ERROR)→ `DispatchOutcome::Completed { status }`。

use std::sync::Arc;

use async_trait::async_trait;
use futures::stream::{self, StreamExt};
use crate::domain::BatchRunMode;
use crate::ports::{DispatchOutcome, PortError, RunTask, TaskDispatcher};
use api_runner::{Assertion, CaseOutcome, ReqwestRunner, RequestSpec};

/// 默认并发上限(PARALLEL 模式下同时在跑的用例数)。
pub const DEFAULT_CONCURRENCY: usize = 8;

/// 一个用例的可执行规格:请求 + 断言。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaseRunSpec {
    pub request: RequestSpec,
    pub assertions: Vec<Assertion>,
}

/// 出站端口:按 case_id 取可执行规格(真实实现查 PG 的 ms_api_case)。
#[async_trait]
pub trait CaseSpecSource: Send + Sync {
    /// 返回该 case_id 的规格;不存在返回 `None`(将被视为该用例失败)。
    async fn spec_of(&self, case_id: &str) -> Result<Option<CaseRunSpec>, PortError>;
}

/// 出站端口:写出单个用例的执行结果明细。
#[async_trait]
pub trait CaseResultSink: Send + Sync {
    async fn record(
        &self,
        report_id: &str,
        case_id: &str,
        outcome: &str,
        failures: &[String],
    ) -> Result<(), PortError>;
}

/// 原生本地执行器。
#[derive(Clone)]
pub struct LocalRunnerDispatcher {
    specs: Arc<dyn CaseSpecSource>,
    sink: Arc<dyn CaseResultSink>,
    runner: ReqwestRunner,
    max_concurrency: usize,
}

impl LocalRunnerDispatcher {
    pub fn new(specs: Arc<dyn CaseSpecSource>, sink: Arc<dyn CaseResultSink>) -> Self {
        // 就地 runner 直连被测主机,默认绕过环境代理(否则 http_proxy 会劫持目标请求)。
        Self { specs, sink, runner: ReqwestRunner::no_proxy(), max_concurrency: DEFAULT_CONCURRENCY }
    }

    pub fn with_concurrency(mut self, n: usize) -> Self {
        self.max_concurrency = n.max(1);
        self
    }

    pub fn with_runner(mut self, runner: ReqwestRunner) -> Self {
        self.runner = runner;
        self
    }

    /// 跑单个用例:执行 + 判定 + 写明细。返回是否通过。
    async fn run_one(&self, report_id: &str, case_id: &str) -> Result<bool, PortError> {
        let (outcome, failures): (&str, Vec<String>) = match self.specs.spec_of(case_id).await? {
            Some(spec) => {
                let report = self.runner.run_case(&spec.request, &spec.assertions).await;
                match report.outcome {
                    CaseOutcome::Success => ("SUCCESS", Vec::new()),
                    CaseOutcome::Error => ("ERROR", report.failures),
                }
            }
            None => ("ERROR", vec![format!("case spec not found: {case_id}")]),
        };
        self.sink.record(report_id, case_id, outcome, &failures).await?;
        Ok(outcome == "SUCCESS")
    }
}

#[async_trait]
impl TaskDispatcher for LocalRunnerDispatcher {
    async fn dispatch_task(&self, task: &RunTask) -> Result<DispatchOutcome, PortError> {
        let results: Vec<Result<bool, PortError>> = match task.mode {
            // 顺序执行
            BatchRunMode::Serial => {
                let mut v = Vec::with_capacity(task.case_ids.len());
                for id in &task.case_ids {
                    v.push(self.run_one(&task.report_id, id).await);
                }
                v
            }
            // 并发执行(并发上限 max_concurrency)。显式收集 future(无闭包),避开闭包返回
            // borrow future 的高阶生命周期限制。
            BatchRunMode::Parallel => {
                let mut futs = Vec::with_capacity(task.case_ids.len());
                for id in &task.case_ids {
                    futs.push(self.run_one(&task.report_id, id));
                }
                stream::iter(futs).buffer_unordered(self.max_concurrency).collect().await
            }
        };

        // 任一基础设施错误直接上抛;否则按是否全过聚合
        let mut all_pass = true;
        for r in results {
            all_pass &= r?;
        }
        let status = if all_pass { "SUCCESS" } else { "ERROR" };
        Ok(DispatchOutcome::Completed { status: status.to_string() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{routing::get, Json, Router};
    use api_runner::HttpMethod;
    use std::collections::HashMap;
    use std::sync::Mutex;
    use tokio::net::TcpListener;

    #[derive(Default)]
    struct InMemorySpecs {
        map: HashMap<String, CaseRunSpec>,
    }
    impl InMemorySpecs {
        fn with(mut self, id: &str, spec: CaseRunSpec) -> Self {
            self.map.insert(id.to_string(), spec);
            self
        }
    }
    #[async_trait]
    impl CaseSpecSource for InMemorySpecs {
        async fn spec_of(&self, case_id: &str) -> Result<Option<CaseRunSpec>, PortError> {
            Ok(self.map.get(case_id).cloned())
        }
    }

    /// 内存结果汇:记录每条 per-case 明细供断言。
    #[derive(Clone, Default)]
    struct SpySink {
        rows: Arc<Mutex<Vec<(String, String, Vec<String>)>>>, // (case_id, outcome, failures)
    }
    #[async_trait]
    impl CaseResultSink for SpySink {
        async fn record(
            &self,
            _report_id: &str,
            case_id: &str,
            outcome: &str,
            failures: &[String],
        ) -> Result<(), PortError> {
            self.rows.lock().expect("lock").push((
                case_id.to_string(),
                outcome.to_string(),
                failures.to_vec(),
            ));
            Ok(())
        }
    }

    async fn spawn() -> String {
        let app = Router::new()
            .route("/ok", get(|| async { Json(serde_json::json!({"status":"ok"})) }))
            .route(
                "/bad",
                get(|| async { (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "boom") }),
            );
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });
        format!("http://{addr}")
    }

    fn spec(url: String, assertions: Vec<Assertion>) -> CaseRunSpec {
        CaseRunSpec {
            request: RequestSpec { method: HttpMethod::Get, url, headers: vec![], body: None },
            assertions,
        }
    }

    fn task(mode: BatchRunMode, case_ids: &[&str]) -> RunTask {
        RunTask {
            report_id: "r1".into(),
            pool_id: "pool1".into(),
            mode,
            case_ids: case_ids.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[tokio::test]
    async fn parallel_all_pass_records_each_case_success() {
        let base = spawn().await;
        let specs = InMemorySpecs::default()
            .with("c1", spec(format!("{base}/ok"), vec![Assertion::StatusIs(200)]))
            .with("c2", spec(format!("{base}/ok"), vec![Assertion::StatusIs(200)]));
        let sink = SpySink::default();
        let d = LocalRunnerDispatcher::new(Arc::new(specs), Arc::new(sink.clone()));

        let outcome =
            d.dispatch_task(&task(BatchRunMode::Parallel, &["c1", "c2"])).await.expect("ok");
        assert_eq!(outcome, DispatchOutcome::Completed { status: "SUCCESS".into() });

        let rows = sink.rows.lock().expect("lock");
        assert_eq!(rows.len(), 2); // 每个用例一条明细
        assert!(rows.iter().all(|(_, o, f)| o == "SUCCESS" && f.is_empty()));
    }

    #[tokio::test]
    async fn failed_case_detail_carries_failure_reasons() {
        let base = spawn().await;
        let specs = InMemorySpecs::default()
            .with("ok", spec(format!("{base}/ok"), vec![Assertion::StatusIs(200)]))
            .with("bad", spec(format!("{base}/bad"), vec![Assertion::StatusIs(200)]));
        let sink = SpySink::default();
        let d = LocalRunnerDispatcher::new(Arc::new(specs), Arc::new(sink.clone()));

        let outcome =
            d.dispatch_task(&task(BatchRunMode::Serial, &["ok", "bad"])).await.expect("ok");
        assert_eq!(outcome, DispatchOutcome::Completed { status: "ERROR".into() });

        let rows = sink.rows.lock().expect("lock");
        let bad = rows.iter().find(|(c, _, _)| c == "bad").expect("bad row");
        assert_eq!(bad.1, "ERROR");
        assert!(!bad.2.is_empty()); // 带失败原因(status 不符)
    }

    #[tokio::test]
    async fn missing_spec_records_error_detail() {
        let sink = SpySink::default();
        let d =
            LocalRunnerDispatcher::new(Arc::new(InMemorySpecs::default()), Arc::new(sink.clone()));
        let outcome =
            d.dispatch_task(&task(BatchRunMode::Parallel, &["ghost"])).await.expect("ok");
        assert_eq!(outcome, DispatchOutcome::Completed { status: "ERROR".into() });
        let rows = sink.rows.lock().expect("lock");
        assert_eq!(rows[0].1, "ERROR");
        assert!(rows[0].2[0].contains("not found"));
    }

    #[tokio::test]
    async fn concurrency_cap_one_still_runs_all() {
        let base = spawn().await;
        let mut specs = InMemorySpecs::default();
        for i in 0..5 {
            specs = specs
                .with(&format!("c{i}"), spec(format!("{base}/ok"), vec![Assertion::StatusIs(200)]));
        }
        let sink = SpySink::default();
        let d = LocalRunnerDispatcher::new(Arc::new(specs), Arc::new(sink.clone()))
            .with_concurrency(1); // 退化为串行也应全跑完
        let ids: Vec<&str> = ["c0", "c1", "c2", "c3", "c4"].to_vec();
        let outcome = d.dispatch_task(&task(BatchRunMode::Parallel, &ids)).await.expect("ok");
        assert_eq!(outcome, DispatchOutcome::Completed { status: "SUCCESS".into() });
        assert_eq!(sink.rows.lock().expect("lock").len(), 5);
    }
}
