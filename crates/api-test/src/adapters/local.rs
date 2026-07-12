use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use crate::domain::{BatchRunMode, ResolvedEnv};
use crate::ports::{DispatchOutcome, EnvVarWriter, PortError, RunTask, TaskDispatcher};
use api_runner::{
    env_extracts, run_extracts, substitute, wait_millis, Assertion, CaseOutcome, Processor,
    RequestSpec, ReqwestRunner, ResponseSnapshot,
};
use async_trait::async_trait;
use futures::stream::{self, StreamExt};

pub fn apply_env_static(req: &mut RequestSpec, env: &ResolvedEnv) {
    let is_absolute = req.url.starts_with("http://") || req.url.starts_with("https://");
    if !env.base_url.is_empty() && !is_absolute {
        let sep = if req.url.is_empty() || req.url.starts_with('/') { "" } else { "/" };
        req.url = format!("{}{sep}{}", env.base_url, req.url);
    }
    // Case headers win (case-insensitive); environment headers only fill gaps.
    for (k, v) in &env.headers {
        if !req.headers.iter().any(|(hk, _)| hk.eq_ignore_ascii_case(k)) {
            req.headers.push((k.clone(), v.clone()));
        }
    }
}

pub(crate) fn substitute_request(req: &mut RequestSpec, vars: &BTreeMap<String, String>) {
    if vars.is_empty() {
        return;
    }
    req.url = substitute(&req.url, vars);
    for (_, v) in req.headers.iter_mut() {
        *v = substitute(v, vars);
    }
    if let Some(b) = req.body.as_mut() {
        *b = substitute(b, vars);
    }
}

pub const DEFAULT_CONCURRENCY: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaseRunSpec {
    pub request: RequestSpec,
    pub assertions: Vec<Assertion>,
    pub processors: Vec<Processor>,
}

#[async_trait]
pub trait CaseSpecSource: Send + Sync {
    async fn spec_of(&self, case_id: &str) -> Result<Option<CaseRunSpec>, PortError>;
}

#[async_trait]
pub trait CaseResultSink: Send + Sync {
    async fn record(
        &self,
        report_id: &str,
        case_id: &str,
        outcome: &str,
        failures: &[String],
    ) -> Result<(), PortError>;

    #[allow(clippy::too_many_arguments)]
    async fn record_detail(
        &self,
        _report_id: &str,
        _case_id: &str,
        _status_code: i32,
        _latency_ms: i64,
        _resp_size: i64,
        _body: &str,
        _headers: &[(String, String)],
        _assertions: &serde_json::Value,
        _extractions: &serde_json::Value,
        _req_method: &str,
        _req_url: &str,
        _req_headers: &[(String, String)],
        _req_body: Option<&str>,
    ) -> Result<(), PortError> {
        Ok(())
    }
}

#[derive(Clone)]
pub struct LocalRunnerDispatcher {
    specs: Arc<dyn CaseSpecSource>,
    sink: Arc<dyn CaseResultSink>,
    runner: ReqwestRunner,
    max_concurrency: usize,
    env_writer: Option<Arc<dyn EnvVarWriter>>,
}

impl LocalRunnerDispatcher {
    pub fn new(specs: Arc<dyn CaseSpecSource>, sink: Arc<dyn CaseResultSink>) -> Self {
        // Bypass environment proxies by default; otherwise http_proxy hijacks requests to the target under test.
        Self {
            specs,
            sink,
            runner: ReqwestRunner::no_proxy(),
            max_concurrency: DEFAULT_CONCURRENCY,
            env_writer: None,
        }
    }

    pub fn with_concurrency(mut self, n: usize) -> Self {
        self.max_concurrency = n.max(1);
        self
    }

    pub fn with_runner(mut self, runner: ReqwestRunner) -> Self {
        self.runner = runner;
        self
    }

    pub fn with_env_writer(mut self, writer: Arc<dyn EnvVarWriter>) -> Self {
        self.env_writer = Some(writer);
        self
    }

    async fn run_one(
        &self,
        report_id: &str,
        case_id: &str,
        env: &ResolvedEnv,
        vars: &BTreeMap<String, String>,
    ) -> Result<(bool, Vec<Processor>, Option<ResponseSnapshot>), PortError> {
        let (outcome, failures, processors, snapshot): (
            &str,
            Vec<String>,
            Vec<Processor>,
            Option<ResponseSnapshot>,
        ) = match self.specs.spec_of(case_id).await? {
            Some(spec) => {
                let mut req = spec.request;
                apply_env_static(&mut req, env);
                substitute_request(&mut req, vars);
                let wait = wait_millis(&spec.processors);
                if wait > 0 {
                    tokio::time::sleep(Duration::from_millis(wait)).await;
                }
                let (report, snap) =
                    self.runner.run_case_with_snapshot_vars(&req, &spec.assertions, vars).await;
                match report.outcome {
                    CaseOutcome::Success => ("SUCCESS", Vec::new(), spec.processors, snap),
                    CaseOutcome::Error => ("ERROR", report.failures, spec.processors, snap),
                }
            }
            None => ("ERROR", vec![format!("case spec not found: {case_id}")], Vec::new(), None),
        };
        self.sink.record(report_id, case_id, outcome, &failures).await?;
        Ok((outcome == "SUCCESS", processors, snapshot))
    }
}

#[async_trait]
impl TaskDispatcher for LocalRunnerDispatcher {
    async fn dispatch_task(&self, task: &RunTask) -> Result<DispatchOutcome, PortError> {
        let results: Vec<Result<bool, PortError>> = match task.mode {
            BatchRunMode::Serial => {
                let mut vars = task.env.variables.clone();
                let mut env_updates: Vec<(String, String)> = Vec::new();
                let mut v = Vec::with_capacity(task.case_ids.len());
                for id in &task.case_ids {
                    match self.run_one(&task.report_id, id, &task.env, &vars).await {
                        Ok((pass, processors, snapshot)) => {
                            if let Some(snap) = &snapshot {
                                for (k, val) in run_extracts(&processors, snap) {
                                    vars.insert(k, val);
                                }
                                if self.env_writer.is_some() && task.environment_id.is_some() {
                                    env_updates.extend(env_extracts(&processors, snap));
                                }
                            }
                            v.push(Ok(pass));
                        }
                        Err(e) => v.push(Err(e)),
                    }
                }
                // Best-effort: writeback failure never affects the case result.
                if let (Some(writer), Some(env_id)) = (&self.env_writer, &task.environment_id) {
                    if !env_updates.is_empty() {
                        if let Err(e) = writer.set_vars(env_id, &env_updates).await {
                            eprintln!("env var writeback failed (env={env_id}): {e:?}");
                        }
                    }
                }
                v
            }
            BatchRunMode::Parallel => {
                let mut futs = Vec::with_capacity(task.case_ids.len());
                for id in &task.case_ids {
                    futs.push(self.run_one(&task.report_id, id, &task.env, &task.env.variables));
                }
                let raw: Vec<Result<(bool, Vec<Processor>, Option<ResponseSnapshot>), PortError>> =
                    stream::iter(futs).buffer_unordered(self.max_concurrency).collect().await;
                raw.into_iter().map(|r| r.map(|(pass, _, _)| pass)).collect()
            }
        };

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
    use api_runner::HttpMethod;
    use axum::{routing::get, Json, Router};
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

    type SinkRow = (String, String, Vec<String>);
    #[derive(Clone, Default)]
    struct SpySink {
        rows: Arc<Mutex<Vec<SinkRow>>>,
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
            self.rows.lock().unwrap_or_else(std::sync::PoisonError::into_inner).push((
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
            processors: vec![],
        }
    }

    fn task(mode: BatchRunMode, case_ids: &[&str]) -> RunTask {
        RunTask {
            report_id: "r1".into(),
            pool_id: "pool1".into(),
            mode,
            case_ids: case_ids.iter().map(|s| s.to_string()).collect(),
            env: ResolvedEnv::default(),
            environment_id: None,
        }
    }

    type WrittenVars = (String, Vec<(String, String)>);
    #[derive(Clone, Default)]
    struct SpyEnvWriter {
        written: Arc<Mutex<Vec<WrittenVars>>>,
    }
    #[async_trait]
    impl EnvVarWriter for SpyEnvWriter {
        async fn set_vars(&self, env_id: &str, vars: &[(String, String)]) -> Result<(), PortError> {
            self.written
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push((env_id.to_string(), vars.to_vec()));
            Ok(())
        }
    }

    #[tokio::test]
    async fn serial_env_scoped_extract_writes_back_to_environment() {
        use api_runner::{ExtractKind, ExtractScope, Extractor};
        let app = Router::new()
            .route("/token", get(|| async { Json(serde_json::json!({"token": "E-7"})) }));
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });
        let a = CaseRunSpec {
            request: RequestSpec {
                method: HttpMethod::Get,
                url: format!("http://{addr}/token"),
                headers: vec![],
                body: None,
            },
            assertions: vec![Assertion::StatusIs(200)],
            processors: vec![Processor::Extract {
                extractors: vec![Extractor {
                    variable: "tk".into(),
                    kind: ExtractKind::JsonPath,
                    expression: "$.token".into(),
                    scope: ExtractScope::Env,
                }],
            }],
        };
        let specs = InMemorySpecs::default().with("a", a);
        let sink = SpySink::default();
        let writer = SpyEnvWriter::default();
        let d = LocalRunnerDispatcher::new(Arc::new(specs), Arc::new(sink))
            .with_env_writer(Arc::new(writer.clone()));
        let mut t = task(BatchRunMode::Serial, &["a"]);
        t.environment_id = Some("env-1".into());
        d.dispatch_task(&t).await.expect("ok");
        let written =
            writer.written.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone();
        assert_eq!(
            written,
            vec![("env-1".to_string(), vec![("tk".to_string(), "E-7".to_string())])]
        );
    }

    #[test]
    fn env_static_inject_then_substitute_with_context_vars() {
        let env = ResolvedEnv {
            base_url: "http://h:1".into(),
            headers: vec![("Authorization".into(), "Bearer ${tok}".into())],
            variables: BTreeMap::new(),
        };
        let mut vars = BTreeMap::new();
        vars.insert("tok".to_string(), "secret".to_string());

        let mut req = RequestSpec {
            method: HttpMethod::Get,
            url: "/api/x".into(),
            headers: vec![],
            body: None,
        };
        apply_env_static(&mut req, &env);
        substitute_request(&mut req, &vars);
        assert_eq!(req.url, "http://h:1/api/x");
        assert_eq!(req.headers, vec![("Authorization".to_string(), "Bearer secret".to_string())]);

        let mut req2 = RequestSpec {
            method: HttpMethod::Get,
            url: "http://other/y".into(),
            headers: vec![("authorization".into(), "keep".into())],
            body: None,
        };
        apply_env_static(&mut req2, &env);
        assert_eq!(req2.url, "http://other/y");
        assert_eq!(req2.headers, vec![("authorization".to_string(), "keep".to_string())]);
    }

    #[tokio::test]
    async fn serial_extract_passes_var_to_next_step() {
        use api_runner::{ExtractKind, Extractor, MatchCondition};
        let app = Router::new()
            .route("/token", get(|| async { Json(serde_json::json!({"token": "T-99"})) }))
            .route(
                "/echo",
                get(|q: axum::extract::Query<HashMap<String, String>>| async move {
                    q.0.get("id").cloned().unwrap_or_default()
                }),
            );
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });
        let base = format!("http://{addr}");

        let a = CaseRunSpec {
            request: RequestSpec {
                method: HttpMethod::Get,
                url: format!("{base}/token"),
                headers: vec![],
                body: None,
            },
            assertions: vec![Assertion::StatusIs(200)],
            processors: vec![Processor::Extract {
                extractors: vec![Extractor {
                    variable: "tk".into(),
                    kind: ExtractKind::JsonPath,
                    expression: "$.token".into(),
                    scope: api_runner::ExtractScope::Temp,
                }],
            }],
        };
        let b = CaseRunSpec {
            request: RequestSpec {
                method: HttpMethod::Get,
                url: format!("{base}/echo?id=${{tk}}"),
                headers: vec![],
                body: None,
            },
            assertions: vec![Assertion::ResponseBody {
                condition: MatchCondition::Equals,
                expected: "T-99".into(),
            }],
            processors: vec![],
        };
        let specs = InMemorySpecs::default().with("a", a).with("b", b);
        let sink = SpySink::default();
        let d = LocalRunnerDispatcher::new(Arc::new(specs), Arc::new(sink.clone()));

        let outcome = d.dispatch_task(&task(BatchRunMode::Serial, &["a", "b"])).await.expect("ok");
        assert_eq!(outcome, DispatchOutcome::Completed { status: "SUCCESS".into() });
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

        let rows = sink.rows.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        assert_eq!(rows.len(), 2);
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

        let rows = sink.rows.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let bad = rows.iter().find(|(c, _, _)| c == "bad").expect("bad row");
        assert_eq!(bad.1, "ERROR");
        assert!(!bad.2.is_empty());
    }

    #[tokio::test]
    async fn missing_spec_records_error_detail() {
        let sink = SpySink::default();
        let d =
            LocalRunnerDispatcher::new(Arc::new(InMemorySpecs::default()), Arc::new(sink.clone()));
        let outcome = d.dispatch_task(&task(BatchRunMode::Parallel, &["ghost"])).await.expect("ok");
        assert_eq!(outcome, DispatchOutcome::Completed { status: "ERROR".into() });
        let rows = sink.rows.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
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
        let d =
            LocalRunnerDispatcher::new(Arc::new(specs), Arc::new(sink.clone())).with_concurrency(1);
        let ids: Vec<&str> = ["c0", "c1", "c2", "c3", "c4"].to_vec();
        let outcome = d.dispatch_task(&task(BatchRunMode::Parallel, &ids)).await.expect("ok");
        assert_eq!(outcome, DispatchOutcome::Completed { status: "SUCCESS".into() });
        assert_eq!(sink.rows.lock().unwrap_or_else(std::sync::PoisonError::into_inner).len(), 5);
    }
}
