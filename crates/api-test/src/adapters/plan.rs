use std::collections::{BTreeMap, HashSet};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use api_runner::{
    evaluate_detailed_with_vars, run_extracts, wait_millis, Assertion, CaseOutcome, MatchCondition,
    Processor, RequestSpec, ReqwestRunner,
};

use super::local::{apply_env_static, substitute_request, CaseResultSink, CaseSpecSource};
use crate::domain::ResolvedEnv;
use crate::ports::PortError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Leaf {
    Case {
        case_id: String,
    },
    Request {
        label: String,
        request: RequestSpec,
        assertions: Vec<Assertion>,
        processors: Vec<Processor>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Condition {
    pub variable: String,
    pub condition: MatchCondition,
    pub value: String,
}

impl Condition {
    pub fn eval(&self, vars: &BTreeMap<String, String>) -> bool {
        let actual = vars.get(&self.variable).map(String::as_str).unwrap_or("");
        self.condition.matches(actual, &self.value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanNode {
    Leaf(Leaf),
    Plugin {
        label: String,
        processor: Option<Processor>,
    },
    Loop {
        times: u32,
        body: Vec<PlanNode>,
    },
    If {
        condition: Condition,
        body: Vec<PlanNode>,
    },
    /// Runs at most once per id for the whole execution (even when wrapped in an outer loop).
    Once {
        id: u32,
        body: Vec<PlanNode>,
    },
    Timer {
        ms: u64,
    },
}

struct RunState {
    vars: BTreeMap<String, String>,
    stopped: bool,
    plugin_executed: bool,
    once_done: HashSet<u32>,
}

/// Step lifecycle hook: lets callers stream live progress (WS relays, remote
/// runners) without touching the result sink. Best-effort, non-async on purpose.
pub trait StepObserver: Send + Sync {
    fn step_started(&self, report_id: &str, step_id: &str);
    fn step_finished(&self, report_id: &str, step_id: &str, outcome: &str, latency_ms: Option<u64>);
}

#[async_trait::async_trait]
pub trait StepPreparation: Send + Sync {
    async fn prepare(
        &self,
        variables: BTreeMap<String, String>,
        processors: &[Processor],
    ) -> Result<BTreeMap<String, String>, String>;
    async fn plugin_step(
        &self,
        _variables: BTreeMap<String, String>,
        _processor: &Processor,
    ) -> Result<BTreeMap<String, String>, String> {
        Err("plugin steps are unavailable in this executor".into())
    }
}

#[derive(Clone)]
pub struct PlanExecutor {
    specs: Arc<dyn CaseSpecSource>,
    sink: Arc<dyn CaseResultSink>,
    runner: ReqwestRunner,
    observer: Option<Arc<dyn StepObserver>>,
    preparation: Option<Arc<dyn StepPreparation>>,
}

impl PlanExecutor {
    pub fn new(specs: Arc<dyn CaseSpecSource>, sink: Arc<dyn CaseResultSink>) -> Self {
        Self { specs, sink, runner: ReqwestRunner::no_proxy(), observer: None, preparation: None }
    }

    pub fn with_preparation(mut self, preparation: Arc<dyn StepPreparation>) -> Self {
        self.preparation = Some(preparation);
        self
    }

    async fn prepare_step(
        &self,
        mut request: RequestSpec,
        env: &ResolvedEnv,
        variables: &BTreeMap<String, String>,
        processors: &[Processor],
        mut assertions: Vec<Assertion>,
    ) -> Result<(RequestSpec, Vec<Assertion>, Option<BTreeMap<String, String>>), String> {
        use api_runner::domain::preparation::{prepare_request, render_assertions};

        if processors
            .iter()
            .any(|processor| matches!(processor, Processor::UnsupportedPostProcessing))
        {
            return Err("plugin, SQL and script processors belong in step preparation".into());
        }
        let preparer = self.preparation.as_ref().ok_or_else(|| {
            "scenario plugin preparation requires an authenticated server run and environment"
                .to_owned()
        })?;
        let preparation: Vec<_> = processors
            .iter()
            .filter(|processor| !matches!(processor, Processor::Extract { .. }))
            .cloned()
            .collect();
        let variables = preparer.prepare(variables.clone(), &preparation).await?;
        let mut headers = env.clone();
        headers.base_url.clear();
        apply_env_static(&mut request, &headers);
        let request = prepare_request(request, Some(&env.base_url), &[], &variables, 1_048_576)
            .map_err(|error| error.to_string())?;
        render_assertions(&mut assertions, &variables, 1_048_576)
            .map_err(|error| error.to_string())?;
        Ok((request, assertions, Some(variables)))
    }

    pub fn with_runner(mut self, runner: ReqwestRunner) -> Self {
        self.runner = runner;
        self
    }

    pub fn with_observer(mut self, observer: Arc<dyn StepObserver>) -> Self {
        self.observer = Some(observer);
        self
    }

    pub async fn run(
        &self,
        report_id: &str,
        plan: &[PlanNode],
        env: &ResolvedEnv,
        stop_on_failure: bool,
    ) -> Result<bool, PortError> {
        let mut state = RunState {
            vars: env.variables.clone(),
            stopped: false,
            plugin_executed: false,
            once_done: HashSet::new(),
        };
        // Built-in per-run variables: `${__runid}` (8-hex, unique per execution)
        // and `${__timestamp}` (epoch ms) let steps mint collision-free names,
        // making chains that create persistent resources safely re-runnable.
        let runid = format!(
            "{:08x}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| (d.as_nanos() as u32) ^ (std::process::id().wrapping_mul(0x9e3779b9)))
                .unwrap_or(0)
        );
        state.vars.entry("__runid".to_string()).or_insert(runid);
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis().to_string())
            .unwrap_or_default();
        state.vars.entry("__timestamp".to_string()).or_insert(ts);
        self.exec_seq(report_id, env, &mut state, plan, stop_on_failure).await
    }

    fn exec_seq<'a>(
        &'a self,
        report_id: &'a str,
        env: &'a ResolvedEnv,
        state: &'a mut RunState,
        nodes: &'a [PlanNode],
        stop_on_failure: bool,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<bool, PortError>> + Send + 'a>> {
        Box::pin(async move {
            let mut all_pass = true;
            for node in nodes {
                if state.stopped {
                    break;
                }
                all_pass &= self.exec_node(report_id, env, state, node, stop_on_failure).await?;
            }
            Ok(all_pass)
        })
    }

    fn exec_node<'a>(
        &'a self,
        report_id: &'a str,
        env: &'a ResolvedEnv,
        state: &'a mut RunState,
        node: &'a PlanNode,
        stop_on_failure: bool,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<bool, PortError>> + Send + 'a>> {
        Box::pin(async move {
            match node {
                PlanNode::Timer { ms } => {
                    if *ms > 0 {
                        tokio::time::sleep(Duration::from_millis(*ms)).await;
                    }
                    Ok(true)
                }
                PlanNode::Loop { times, body } => {
                    let mut all_pass = true;
                    for _ in 0..*times {
                        if state.stopped {
                            break;
                        }
                        all_pass &=
                            self.exec_seq(report_id, env, state, body, stop_on_failure).await?;
                    }
                    Ok(all_pass)
                }
                PlanNode::If { condition, body } => {
                    if condition.eval(&state.vars) {
                        self.exec_seq(report_id, env, state, body, stop_on_failure).await
                    } else {
                        Ok(true)
                    }
                }
                PlanNode::Once { id, body } => {
                    if state.once_done.insert(*id) {
                        self.exec_seq(report_id, env, state, body, stop_on_failure).await
                    } else {
                        Ok(true)
                    }
                }
                PlanNode::Plugin { label, processor } => {
                    self.exec_plugin(report_id, state, label, processor.as_ref(), stop_on_failure)
                        .await
                }
                PlanNode::Leaf(leaf) => {
                    self.exec_leaf(report_id, env, state, leaf, stop_on_failure).await
                }
            }
        })
    }

    async fn exec_plugin(
        &self,
        report_id: &str,
        state: &mut RunState,
        label: &str,
        processor: Option<&Processor>,
        stop_on_failure: bool,
    ) -> Result<bool, PortError> {
        if let Some(observer) = &self.observer {
            observer.step_started(report_id, label);
        }
        let started = std::time::Instant::now();
        state.plugin_executed = true;
        let result = match (processor, &self.preparation) {
            (
                Some(
                    processor @ (Processor::Sql { name, .. }
                    | Processor::SandboxScript { output: name, .. }),
                ),
                Some(preparer),
            ) => {
                let result = preparer.plugin_step(state.vars.clone(), processor).await;
                state.vars.remove(name);
                result.and_then(|mut variables| {
                    variables.remove(name).map(|value| (name.clone(), value)).ok_or_else(|| {
                        "plugin did not return the configured output variable".into()
                    })
                })
            }
            (
                Some(Processor::Sql { name, .. } | Processor::SandboxScript { output: name, .. }),
                None,
            ) => {
                state.vars.remove(name);
                Err("plugin step requires an authenticated server run and environment".into())
            }
            _ => Err("invalid plugin step configuration".into()),
        };
        let elapsed = started.elapsed().as_millis() as u64;
        let (outcome, failures) = match result {
            Ok((name, value)) => {
                state.vars.insert(name, value);
                ("SUCCESS", Vec::new())
            }
            Err(error) => ("ERROR", vec![error]),
        };
        self.sink.record(report_id, label, outcome, &failures).await?;
        let empty = serde_json::json!([]);
        self.sink
            .record_detail(
                report_id,
                label,
                0,
                elapsed as i64,
                0,
                "",
                &[],
                &empty,
                &empty,
                label.split_once(' ').map_or("PLUGIN", |(kind, _)| kind),
                label.split_once(' ').map_or(label, |(_, name)| name),
                &[],
                None,
                &serde_json::Value::Null,
            )
            .await?;
        if let Some(observer) = &self.observer {
            observer.step_finished(report_id, label, outcome, Some(elapsed));
        }
        let passed = outcome == "SUCCESS";
        if !passed && stop_on_failure {
            state.stopped = true;
        }
        Ok(passed)
    }

    async fn exec_leaf(
        &self,
        report_id: &str,
        env: &ResolvedEnv,
        state: &mut RunState,
        leaf: &Leaf,
        stop_on_failure: bool,
    ) -> Result<bool, PortError> {
        let step_id = match leaf {
            Leaf::Case { case_id } => case_id.clone(),
            Leaf::Request { label, .. } => label.clone(),
        };
        if let Some(obs) = &self.observer {
            obs.step_started(report_id, &step_id);
        }
        let (case_id, mut req, mut assertions, processors) = match leaf {
            Leaf::Case { case_id } => match self.specs.spec_of(case_id).await? {
                Some(spec) => (case_id.clone(), spec.request, spec.assertions, spec.processors),
                None => {
                    self.sink
                        .record(
                            report_id,
                            case_id,
                            "ERROR",
                            &[format!("case spec not found: {case_id}")],
                        )
                        .await?;
                    if let Some(obs) = &self.observer {
                        obs.step_finished(report_id, &step_id, "ERROR", None);
                    }
                    if stop_on_failure {
                        state.stopped = true;
                    }
                    return Ok(false);
                }
            },
            Leaf::Request { label, request, assertions, processors } => {
                (label.clone(), request.clone(), assertions.clone(), processors.clone())
            }
        };

        let prepared = if api_runner::domain::preparation::needs_plugin_preparation(&processors) {
            self.prepare_step(req, env, &state.vars, &processors, assertions).await
        } else {
            apply_env_static(&mut req, env);
            let wait = wait_millis(&processors);
            if wait > 0 {
                tokio::time::sleep(Duration::from_millis(wait)).await;
            }
            if state.plugin_executed {
                use api_runner::domain::preparation::{prepare_request, render_assertions};
                prepare_request(req, None, &[], &state.vars, 1_048_576)
                    .and_then(|request| {
                        render_assertions(&mut assertions, &state.vars, 1_048_576)?;
                        Ok((request, assertions, None))
                    })
                    .map_err(|error| error.to_string())
            } else {
                substitute_request(&mut req, &state.vars);
                Ok((req, assertions, None))
            }
        };
        let (req, assertions, prepared_vars) = match prepared {
            Ok(prepared) => prepared,
            Err(error) => {
                self.sink.record(report_id, &case_id, "ERROR", &[error]).await?;
                if let Some(obs) = &self.observer {
                    obs.step_finished(report_id, &step_id, "ERROR", None);
                }
                if stop_on_failure {
                    state.stopped = true;
                }
                return Ok(false);
            }
        };
        let variables = prepared_vars.as_ref().unwrap_or(&state.vars);
        // Must pass state.vars: otherwise the outcome evaluates Variable assertions with empty vars, misreporting and contradicting `detailed` below.
        let (report, snap) =
            self.runner.run_case_with_snapshot_vars(&req, &assertions, variables).await;
        let (outcome, failures): (&str, Vec<String>) = match report.outcome {
            CaseOutcome::Success => ("SUCCESS", Vec::new()),
            CaseOutcome::Error => ("ERROR", report.failures),
        };
        self.sink.record(report_id, &case_id, outcome, &failures).await?;
        if let Some(obs) = &self.observer {
            obs.step_finished(report_id, &step_id, outcome, snap.as_ref().map(|s| s.elapsed_ms));
        }
        // Best-effort: writeback failure never affects execution. Extraction is computed once, then both persisted and written to the context.
        if let Some(s) = &snap {
            let detailed = evaluate_detailed_with_vars(&assertions, s, variables);
            let assertions_json = serde_json::Value::Array(
                detailed
                    .iter()
                    .map(|a| {
                        serde_json::json!({
                            "item": a.item, "condition": a.condition, "expected": a.expected,
                            "actual": a.actual, "passed": a.passed, "reason": a.reason,
                        })
                    })
                    .collect(),
            );
            let extracts = run_extracts(&processors, s);
            let extractions_json = serde_json::Value::Array(
                extracts.iter().map(|(k, v)| serde_json::json!([k, v])).collect(),
            );
            let _ = self
                .sink
                .record_detail(
                    report_id,
                    &case_id,
                    s.status as i32,
                    s.elapsed_ms as i64,
                    s.body.len() as i64,
                    &s.body,
                    &s.headers,
                    &assertions_json,
                    &extractions_json,
                    req.method.as_str(),
                    &req.url,
                    &req.headers,
                    req.body.as_deref(),
                    &serde_json::to_value(s.timings).unwrap_or(serde_json::Value::Null),
                )
                .await;
            for (k, v) in extracts {
                state.vars.insert(k, v);
            }
        }
        let pass = outcome == "SUCCESS";
        if !pass && stop_on_failure {
            state.stopped = true;
        }
        Ok(pass)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::local::{CaseResultSink, CaseRunSpec, CaseSpecSource};
    use api_runner::HttpMethod;
    use async_trait::async_trait;
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

    #[derive(Clone, Default)]
    struct SpySink {
        rows: Arc<Mutex<Vec<(String, String)>>>,
    }
    #[async_trait]
    impl CaseResultSink for SpySink {
        async fn record(
            &self,
            _report_id: &str,
            case_id: &str,
            outcome: &str,
            _failures: &[String],
        ) -> Result<(), PortError> {
            self.rows
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push((case_id.to_string(), outcome.to_string()));
            Ok(())
        }
    }

    async fn spawn() -> String {
        let app = Router::new()
            .route("/ok", get(|| async { Json(serde_json::json!({"ok": true})) }))
            .route("/token", get(|| async { Json(serde_json::json!({"token": "T-1"})) }))
            .route(
                "/echo",
                get(|q: axum::extract::Query<HashMap<String, String>>| async move {
                    q.0.get("id").cloned().unwrap_or_default()
                }),
            );
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn plan_runner_rejects_unwired_scripts_before_http() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = calls.clone();
        let app = Router::new().fallback(move || {
            let counter = counter.clone();
            async move {
                counter.fetch_add(1, Ordering::SeqCst);
                "ok"
            }
        });
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let address = listener.local_addr().expect("address");
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("server");
        });
        let sink = SpySink::default();
        let plan = vec![PlanNode::Leaf(Leaf::Request {
            label: "hook".into(),
            request: RequestSpec {
                timeout_ms: None,
                method: HttpMethod::Get,
                url: format!("http://{address}"),
                headers: vec![],
                body: None,
            },
            assertions: vec![],
            processors: vec![Processor::Script { lang: "JS".into(), code: "vars.x='ok'".into() }],
        })];
        let passed = exec(InMemorySpecs::default(), sink.clone())
            .run("report", &plan, &ResolvedEnv::default(), true)
            .await
            .expect("record failure");
        assert!(!passed);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert_eq!(sink.rows.lock().expect("rows")[0].1, "ERROR");
        server.abort();
    }
    struct SqlPreparationFixture;

    #[async_trait]
    impl StepPreparation for SqlPreparationFixture {
        async fn prepare(
            &self,
            mut variables: BTreeMap<String, String>,
            processors: &[Processor],
        ) -> Result<BTreeMap<String, String>, String> {
            let [Processor::Sql { name, .. }] = processors else {
                return Err("expected one SQL processor".into());
            };
            variables.insert(name.clone(), r#"{"rows":[[42,"20261121"]]}"#.into());
            Ok(variables)
        }
        async fn plugin_step(
            &self,
            variables: BTreeMap<String, String>,
            processor: &Processor,
        ) -> Result<BTreeMap<String, String>, String> {
            self.prepare(variables, std::slice::from_ref(processor)).await
        }
    }

    #[tokio::test]
    async fn sql_preparation_supplies_request_and_response_assertion_values() {
        use axum::extract::Path;
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("listen");
        let address = listener.local_addr().expect("address");
        let app = Router::new().route(
            "/snapshot/{id}",
            get(|Path(id): Path<String>| async move {
                Json(serde_json::json!({"id": id, "count": 42}))
            }),
        );
        let server = tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });
        let sink = SpySink::default();
        let spec = get_case(
            "/snapshot/{{baseline#/rows/0/1}}".into(),
            vec![
                Assertion::JsonFieldEquals {
                    pointer: "/count".into(),
                    expected: "${baseline#/rows/0/0}".into(),
                },
                Assertion::JsonFieldEquals { pointer: "/id".into(), expected: "20261121".into() },
            ],
            vec![Processor::Sql {
                name: "baseline".into(),
                datasource: "postgres".into(),
                sql: "SELECT 42".into(),
            }],
        );
        let executor = exec(InMemorySpecs::default().with("case", spec), sink.clone())
            .with_preparation(Arc::new(SqlPreparationFixture));
        let plan = [PlanNode::Leaf(Leaf::Case { case_id: "case".into() })];
        let env = ResolvedEnv { base_url: format!("http://{address}"), ..Default::default() };
        assert!(executor.run("report", &plan, &env, true).await.expect("run"));
        assert_eq!(sink.rows.lock().expect("rows")[0].1, "SUCCESS");
        server.abort();
    }

    #[tokio::test]
    async fn plugin_output_is_available_to_the_next_request() {
        let base = spawn().await;
        let sink = SpySink::default();
        let specs = InMemorySpecs::default().with(
            "next",
            get_case(
                format!("{base}/echo?id={{{{baseline#/rows/0/0}}}}"),
                vec![Assertion::BodyContains("{{baseline#/rows/0/0}}".into())],
                vec![],
            ),
        );
        let plan = [
            PlanNode::Plugin {
                label: "PLUGIN baseline".into(),
                processor: Some(Processor::Sql {
                    name: "baseline".into(),
                    datasource: "postgres".into(),
                    sql: "SELECT 42".into(),
                }),
            },
            PlanNode::Leaf(Leaf::Case { case_id: "next".into() }),
        ];
        let executor = exec(specs, sink.clone()).with_preparation(Arc::new(SqlPreparationFixture));
        assert!(executor.run("report", &plan, &ResolvedEnv::default(), true).await.expect("run"));
        assert_eq!(
            *sink.rows.lock().expect("rows"),
            vec![("PLUGIN baseline".into(), "SUCCESS".into()), ("next".into(), "SUCCESS".into()),]
        );
        let missing = SpySink::default();
        assert!(!exec(InMemorySpecs::default(), missing.clone())
            .run("report", &plan, &ResolvedEnv::default(), true)
            .await
            .expect("run"));
        assert_eq!(
            *missing.rows.lock().expect("rows"),
            vec![("PLUGIN baseline".into(), "ERROR".into())]
        );
    }

    fn get_case(
        url: String,
        assertions: Vec<Assertion>,
        processors: Vec<Processor>,
    ) -> CaseRunSpec {
        CaseRunSpec {
            request: RequestSpec {
                timeout_ms: None,
                method: HttpMethod::Get,
                url,
                headers: vec![],
                body: None,
            },
            assertions,
            processors,
        }
    }

    fn exec(specs: InMemorySpecs, sink: SpySink) -> PlanExecutor {
        PlanExecutor::new(Arc::new(specs), Arc::new(sink))
    }

    #[test]
    fn condition_eval_uses_context_vars() {
        let mut vars = BTreeMap::new();
        vars.insert("role".to_string(), "admin".to_string());
        let c = Condition {
            variable: "role".into(),
            condition: MatchCondition::Equals,
            value: "admin".into(),
        };
        assert!(c.eval(&vars));
        let miss = Condition {
            variable: "ghost".into(),
            condition: MatchCondition::NotEmpty,
            value: "".into(),
        };
        assert!(!miss.eval(&vars));
    }

    #[tokio::test]
    async fn loop_repeats_body_n_times() {
        let base = spawn().await;
        let specs = InMemorySpecs::default()
            .with("c", get_case(format!("{base}/ok"), vec![Assertion::StatusIs(200)], vec![]));
        let sink = SpySink::default();
        let plan = vec![PlanNode::Loop {
            times: 3,
            body: vec![PlanNode::Leaf(Leaf::Case { case_id: "c".into() })],
        }];
        let ok = exec(specs, sink.clone())
            .run("r1", &plan, &ResolvedEnv::default(), false)
            .await
            .expect("ok");
        assert!(ok);
        assert_eq!(sink.rows.lock().unwrap_or_else(std::sync::PoisonError::into_inner).len(), 3);
    }

    #[tokio::test]
    async fn if_true_runs_else_skips() {
        let base = spawn().await;
        let specs = InMemorySpecs::default()
            .with("c", get_case(format!("{base}/ok"), vec![Assertion::StatusIs(200)], vec![]));
        let sink = SpySink::default();
        let env = ResolvedEnv {
            base_url: String::new(),
            headers: vec![],
            variables: [("go".to_string(), "yes".to_string())].into_iter().collect(),
        };
        let plan = vec![
            PlanNode::If {
                condition: Condition {
                    variable: "go".into(),
                    condition: MatchCondition::Equals,
                    value: "yes".into(),
                },
                body: vec![PlanNode::Leaf(Leaf::Case { case_id: "c".into() })],
            },
            PlanNode::If {
                condition: Condition {
                    variable: "go".into(),
                    condition: MatchCondition::Equals,
                    value: "no".into(),
                },
                body: vec![PlanNode::Leaf(Leaf::Case { case_id: "c".into() })],
            },
        ];
        exec(specs, sink.clone()).run("r1", &plan, &env, false).await.expect("ok");
        assert_eq!(sink.rows.lock().unwrap_or_else(std::sync::PoisonError::into_inner).len(), 1);
    }

    #[tokio::test]
    async fn once_inside_loop_runs_single_time() {
        let base = spawn().await;
        let specs = InMemorySpecs::default()
            .with("body", get_case(format!("{base}/ok"), vec![Assertion::StatusIs(200)], vec![]))
            .with("once", get_case(format!("{base}/ok"), vec![Assertion::StatusIs(200)], vec![]));
        let sink = SpySink::default();
        let plan = vec![PlanNode::Loop {
            times: 3,
            body: vec![
                PlanNode::Leaf(Leaf::Case { case_id: "body".into() }),
                PlanNode::Once {
                    id: 1,
                    body: vec![PlanNode::Leaf(Leaf::Case { case_id: "once".into() })],
                },
            ],
        }];
        exec(specs, sink.clone())
            .run("r1", &plan, &ResolvedEnv::default(), false)
            .await
            .expect("ok");
        let rows = sink.rows.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        assert_eq!(rows.iter().filter(|(id, _)| id == "body").count(), 3);
        assert_eq!(rows.iter().filter(|(id, _)| id == "once").count(), 1);
    }

    #[tokio::test]
    async fn stop_on_failure_halts_remaining() {
        let base = spawn().await;
        let specs = InMemorySpecs::default()
            .with("bad", get_case(format!("{base}/ok"), vec![Assertion::StatusIs(500)], vec![]))
            .with("after", get_case(format!("{base}/ok"), vec![Assertion::StatusIs(200)], vec![]));
        let sink = SpySink::default();
        let plan = vec![
            PlanNode::Leaf(Leaf::Case { case_id: "bad".into() }),
            PlanNode::Leaf(Leaf::Case { case_id: "after".into() }),
        ];
        let ok = exec(specs, sink.clone())
            .run("r1", &plan, &ResolvedEnv::default(), true)
            .await
            .expect("ok");
        assert!(!ok);
        let rows = sink.rows.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0], ("bad".to_string(), "ERROR".to_string()));
    }

    #[tokio::test]
    async fn extract_in_plan_passes_var_to_later_leaf() {
        let base = spawn().await;
        let extract = vec![Processor::Extract {
            extractors: vec![api_runner::Extractor {
                variable: "tk".into(),
                kind: api_runner::ExtractKind::JsonPath,
                expression: "$.token".into(),
                scope: api_runner::ExtractScope::Temp,
            }],
        }];
        let specs = InMemorySpecs::default()
            .with("a", get_case(format!("{base}/token"), vec![Assertion::StatusIs(200)], extract))
            .with(
                "b",
                get_case(
                    format!("{base}/echo?id=${{tk}}"),
                    vec![Assertion::ResponseBody {
                        condition: MatchCondition::Equals,
                        expected: "T-1".into(),
                    }],
                    vec![],
                ),
            );
        let sink = SpySink::default();
        let plan = vec![
            PlanNode::Leaf(Leaf::Case { case_id: "a".into() }),
            PlanNode::Leaf(Leaf::Case { case_id: "b".into() }),
        ];
        let ok = exec(specs, sink.clone())
            .run("r1", &plan, &ResolvedEnv::default(), false)
            .await
            .expect("ok");
        assert!(ok);
    }

    #[tokio::test]
    async fn case_variable_assertion_uses_context_vars_not_misreport() {
        let base = spawn().await;
        let extract = vec![Processor::Extract {
            extractors: vec![api_runner::Extractor {
                variable: "tk".into(),
                kind: api_runner::ExtractKind::JsonPath,
                expression: "$.token".into(),
                scope: api_runner::ExtractScope::Temp,
            }],
        }];
        let specs = InMemorySpecs::default()
            .with("a", get_case(format!("{base}/token"), vec![Assertion::StatusIs(200)], extract))
            .with(
                "b",
                get_case(
                    format!("{base}/ok"),
                    vec![Assertion::Variable {
                        name: "tk".into(),
                        condition: MatchCondition::Equals,
                        expected: "T-1".into(),
                    }],
                    vec![],
                ),
            );
        let sink = SpySink::default();
        let plan = vec![
            PlanNode::Leaf(Leaf::Case { case_id: "a".into() }),
            PlanNode::Leaf(Leaf::Case { case_id: "b".into() }),
        ];
        let ok = exec(specs, sink.clone())
            .run("r1", &plan, &ResolvedEnv::default(), false)
            .await
            .expect("ok");
        assert!(ok);
        let rows = sink.rows.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let b = rows.iter().find(|(id, _)| id == "b").expect("b recorded");
        assert_eq!(b.1, "SUCCESS");
    }
}
