use std::collections::{BTreeMap, HashSet};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use api_runner::{
    evaluate_detailed_with_vars, run_extracts, wait_millis, Assertion, CaseOutcome, MatchCondition,
    Processor, ReqwestRunner,
    RequestSpec,
};

use super::local::{apply_env_static, substitute_request, CaseResultSink, CaseSpecSource};
use crate::domain::ResolvedEnv;
use crate::ports::PortError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Leaf {
    Case { case_id: String },
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
    Loop { times: u32, body: Vec<PlanNode> },
    If { condition: Condition, body: Vec<PlanNode> },
    /// 同一 id 全程只进一次(被外层循环包裹也只跑首次)。
    Once { id: u32, body: Vec<PlanNode> },
    Timer { ms: u64 },
}

struct RunState {
    vars: BTreeMap<String, String>,
    stopped: bool,
    once_done: HashSet<u32>,
}

#[derive(Clone)]
pub struct PlanExecutor {
    specs: Arc<dyn CaseSpecSource>,
    sink: Arc<dyn CaseResultSink>,
    runner: ReqwestRunner,
}

impl PlanExecutor {
    pub fn new(specs: Arc<dyn CaseSpecSource>, sink: Arc<dyn CaseResultSink>) -> Self {
        Self { specs, sink, runner: ReqwestRunner::no_proxy() }
    }

    pub fn with_runner(mut self, runner: ReqwestRunner) -> Self {
        self.runner = runner;
        self
    }

    pub async fn run(
        &self,
        report_id: &str,
        plan: &[PlanNode],
        env: &ResolvedEnv,
        stop_on_failure: bool,
    ) -> Result<bool, PortError> {
        let mut state =
            RunState { vars: env.variables.clone(), stopped: false, once_done: HashSet::new() };
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
                PlanNode::Leaf(leaf) => {
                    self.exec_leaf(report_id, env, state, leaf, stop_on_failure).await
                }
            }
        })
    }

    async fn exec_leaf(
        &self,
        report_id: &str,
        env: &ResolvedEnv,
        state: &mut RunState,
        leaf: &Leaf,
        stop_on_failure: bool,
    ) -> Result<bool, PortError> {
        let (case_id, mut req, assertions, processors) = match leaf {
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

        apply_env_static(&mut req, env);
        substitute_request(&mut req, &state.vars);
        let wait = wait_millis(&processors);
        if wait > 0 {
            tokio::time::sleep(Duration::from_millis(wait)).await;
        }
        // 必须传 state.vars:否则 outcome 用空 vars 算 Variable 断言会误报,与下方 detailed 矛盾。
        let (report, snap) =
            self.runner.run_case_with_snapshot_vars(&req, &assertions, &state.vars).await;
        let (outcome, failures): (&str, Vec<String>) = match report.outcome {
            CaseOutcome::Success => ("SUCCESS", Vec::new()),
            CaseOutcome::Error => ("ERROR", report.failures),
        };
        self.sink.record(report_id, &case_id, outcome, &failures).await?;
        // best-effort:回填失败不影响执行。提取只算一次,既落库又写上下文。
        if let Some(s) = &snap {
            let detailed = evaluate_detailed_with_vars(&assertions, s, &state.vars);
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
            self.rows.lock().unwrap_or_else(std::sync::PoisonError::into_inner).push((case_id.to_string(), outcome.to_string()));
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

    fn get_case(url: String, assertions: Vec<Assertion>, processors: Vec<Processor>) -> CaseRunSpec {
        CaseRunSpec {
            request: RequestSpec { method: HttpMethod::Get, url, headers: vec![], body: None },
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
        let specs =
            InMemorySpecs::default().with("c", get_case(format!("{base}/ok"), vec![Assertion::StatusIs(200)], vec![]));
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
        let specs =
            InMemorySpecs::default().with("c", get_case(format!("{base}/ok"), vec![Assertion::StatusIs(200)], vec![]));
        let sink = SpySink::default();
        let env = ResolvedEnv {
            base_url: String::new(),
            headers: vec![],
            variables: [("go".to_string(), "yes".to_string())].into_iter().collect(),
        };
        let plan = vec![
            PlanNode::If {
                condition: Condition { variable: "go".into(), condition: MatchCondition::Equals, value: "yes".into() },
                body: vec![PlanNode::Leaf(Leaf::Case { case_id: "c".into() })],
            },
            PlanNode::If {
                condition: Condition { variable: "go".into(), condition: MatchCondition::Equals, value: "no".into() },
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
                PlanNode::Once { id: 1, body: vec![PlanNode::Leaf(Leaf::Case { case_id: "once".into() })] },
            ],
        }];
        exec(specs, sink.clone()).run("r1", &plan, &ResolvedEnv::default(), false).await.expect("ok");
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
                    vec![Assertion::ResponseBody { condition: MatchCondition::Equals, expected: "T-1".into() }],
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
