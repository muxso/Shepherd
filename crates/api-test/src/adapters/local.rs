//! 原生本地执行器:实现 `crate::ports::TaskDispatcher`,用 `api-runner`
//! 就地跑用例,**完全不依赖 JMeter**。
//!
//! 能力:
//!  - 取每个 case 的请求规格+断言(`CaseSpecSource`)→ reqwest 执行+判定;
//!  - **per-case 结果明细**写出(`CaseResultSink`:成功/失败 + 失败原因列表);
//!  - **并发执行**:PARALLEL 模式按并发上限并行跑,SERIAL 顺序跑;
//!  - 聚合为整体状态(任一失败/缺失即 ERROR)→ `DispatchOutcome::Completed { status }`。

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures::stream::{self, StreamExt};
use crate::domain::{BatchRunMode, ResolvedEnv};
use crate::ports::{DispatchOutcome, EnvVarWriter, PortError, RunTask, TaskDispatcher};
use api_runner::{
    env_extracts, run_extracts, substitute, wait_millis, Assertion, CaseOutcome, Processor,
    ReqwestRunner, RequestSpec, ResponseSnapshot,
};

/// 静态环境注入:相对 url 拼 base_url、并入默认头(已有同名不覆盖)。**不**做变量替换
/// (替换交给 `substitute_request`,用运行时上下文变量,而非仅环境变量)。
pub fn apply_env_static(req: &mut RequestSpec, env: &ResolvedEnv) {
    // base_url:仅对相对 url 生效(绝对 url 原样)。
    let is_absolute = req.url.starts_with("http://") || req.url.starts_with("https://");
    if !env.base_url.is_empty() && !is_absolute {
        let sep = if req.url.is_empty() || req.url.starts_with('/') { "" } else { "/" };
        req.url = format!("{}{sep}{}", env.base_url, req.url);
    }
    // 默认头:用例已有同名头(忽略大小写)优先,环境只补缺。
    for (k, v) in &env.headers {
        if !req.headers.iter().any(|(hk, _)| hk.eq_ignore_ascii_case(k)) {
            req.headers.push((k.clone(), v.clone()));
        }
    }
}

/// 用运行上下文变量对 url / 各头值 / body 做 `${var}` 替换。空表直接返回。
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

/// 默认并发上限(PARALLEL 模式下同时在跑的用例数)。
pub const DEFAULT_CONCURRENCY: usize = 8;

/// 一个用例的可执行规格:请求 + 断言 + 前后置处理器。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaseRunSpec {
    pub request: RequestSpec,
    pub assertions: Vec<Assertion>,
    /// 前后置处理器(WAIT 前置 sleep、EXTRACT 后置写变量);串行执行时生效。
    pub processors: Vec<Processor>,
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

    /// 写出响应明细(状态码/耗时/大小/响应头/响应体);best-effort。默认 no-op,仅持久化实现覆盖。
    /// 在 [`CaseResultSink::record`] 之后调用(更新同一行)。
    async fn record_detail(
        &self,
        _report_id: &str,
        _case_id: &str,
        _status_code: i32,
        _latency_ms: i64,
        _resp_size: i64,
        _body: &str,
        _headers: &[(String, String)],
    ) -> Result<(), PortError> {
        Ok(())
    }
}

/// 原生本地执行器。
#[derive(Clone)]
pub struct LocalRunnerDispatcher {
    specs: Arc<dyn CaseSpecSource>,
    sink: Arc<dyn CaseResultSink>,
    runner: ReqwestRunner,
    max_concurrency: usize,
    /// 「环境参数」提取回写端口;None 则不回写(默认)。
    env_writer: Option<Arc<dyn EnvVarWriter>>,
}

impl LocalRunnerDispatcher {
    pub fn new(specs: Arc<dyn CaseSpecSource>, sink: Arc<dyn CaseResultSink>) -> Self {
        // 就地 runner 直连被测主机,默认绕过环境代理(否则 http_proxy 会劫持目标请求)。
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

    /// 注入「环境参数」回写端口(串行执行时,EXTRACT 的 Env 作用域结果写回该环境)。
    pub fn with_env_writer(mut self, writer: Arc<dyn EnvVarWriter>) -> Self {
        self.env_writer = Some(writer);
        self
    }

    /// 跑单个用例:静态环境注入 → 变量替换 → WAIT 前置 → 执行判定 → 写明细。
    /// 返回 `(是否通过, 该用例的处理器, 响应快照)`,后两者供串行 RunContext 做 EXTRACT。
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
                // WAIT 前置:think-time。
                let wait = wait_millis(&spec.processors);
                if wait > 0 {
                    tokio::time::sleep(Duration::from_millis(wait)).await;
                }
                // 带运行上下文变量执行,使 Variable 断言可读取已提取/环境变量。
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
            // 顺序执行 + RunContext:环境变量作种子,EXTRACT 后置把提取值写入上下文,
            // 后续步骤的 ${var} 即可引用(数据驱动/跨步传参)。
            BatchRunMode::Serial => {
                let mut vars = task.env.variables.clone();
                // 「环境参数」作用域的提取结果累积,串行跑完统一回写环境(一次写)。
                let mut env_updates: Vec<(String, String)> = Vec::new();
                let mut v = Vec::with_capacity(task.case_ids.len());
                for id in &task.case_ids {
                    match self.run_one(&task.report_id, id, &task.env, &vars).await {
                        Ok((pass, processors, snapshot)) => {
                            if let Some(snap) = &snapshot {
                                // 全部提取进运行上下文(供后续步骤 ${var});Env 作用域另收集回写。
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
                // 回写环境变量(best-effort:失败不影响用例结果,仅记日志)。
                if let (Some(writer), Some(env_id)) = (&self.env_writer, &task.environment_id) {
                    if !env_updates.is_empty() {
                        if let Err(e) = writer.set_vars(env_id, &env_updates).await {
                            eprintln!("env var writeback failed (env={env_id}): {e:?}");
                        }
                    }
                }
                v
            }
            // 并发执行:各用例独立、无共享上下文,仅用环境变量替换;EXTRACT 不跨用例传递。
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

    /// 间谍:记录被回写的环境变量(用于校验「环境参数」回写)。
    #[derive(Clone, Default)]
    struct SpyEnvWriter {
        written: Arc<Mutex<Vec<(String, Vec<(String, String)>)>>>,
    }
    #[async_trait]
    impl EnvVarWriter for SpyEnvWriter {
        async fn set_vars(&self, env_id: &str, vars: &[(String, String)]) -> Result<(), PortError> {
            self.written.lock().expect("lock").push((env_id.to_string(), vars.to_vec()));
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
        // 用例:GET /token,后置 EXTRACT $.token → 变量 tk,作用域 Env
        let a = CaseRunSpec {
            request: RequestSpec { method: HttpMethod::Get, url: format!("http://{addr}/token"), headers: vec![], body: None },
            assertions: vec![Assertion::StatusIs(200)],
            processors: vec![Processor::Extract {
                extractors: vec![Extractor { variable: "tk".into(), kind: ExtractKind::JsonPath, expression: "$.token".into(), scope: ExtractScope::Env }],
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
        // 断言:Env 作用域提取结果回写到 env-1。
        let written = writer.written.lock().expect("lock").clone();
        assert_eq!(written, vec![("env-1".to_string(), vec![("tk".to_string(), "E-7".to_string())])]);
    }

    #[test]
    fn env_static_inject_then_substitute_with_context_vars() {
        let env = ResolvedEnv {
            base_url: "http://h:1".into(),
            headers: vec![("Authorization".into(), "Bearer ${tok}".into())],
            variables: BTreeMap::new(),
        };
        // 上下文变量(可来自环境种子或上一步 EXTRACT)
        let mut vars = BTreeMap::new();
        vars.insert("tok".to_string(), "secret".to_string());

        // 相对 url + 无头:static 注入 base+头,再用上下文变量替换
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

        // 绝对 url 不前缀;用例已有同名头(忽略大小写)优先,环境不覆盖
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
        // mock:/token 返回 token;/echo 回显 query 参数 id
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

        // 用例 A:GET /token,后置 EXTRACT $.token → 变量 tk
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
        // 用例 B:GET /echo?id=${tk},断言回显体等于上一步提取的 T-99
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

        // 串行:A 提取 tk → B 的 ${tk} 替换为 T-99 → 断言通过
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
