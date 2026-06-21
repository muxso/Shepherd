//! 计划树执行器:带运行上下文顺序走「控制器 + 叶子」树。
//!
//! 这是场景「逻辑控制」的执行核心——把扁平串行执行升级为可嵌套的树:
//!  - `Loop`   循环控制器(定次 LOOP_COUNT)
//!  - `If`     条件控制器(按上下文变量 + 操作符判定是否进入分支)
//!  - `Once`   仅一次控制器(全程只进一次,即便被外层循环包裹)
//!  - `Timer`  等待控制器(sleep)
//!  - `Leaf`   叶子:引用用例(CASE)或内联请求(REQUEST)
//!
//! 复用 [`super::local`] 的环境注入 / 变量替换 / 结果汇:叶子执行 = 环境静态注入 →
//! `${var}` 替换(用 live 上下文)→ WAIT 前置 → 执行判定 → 写明细 → EXTRACT 后置写变量。
//! `failureStrategy`:`stop_on_failure=true` 时,任一叶子失败即停止后续(对应场景 SETTING 的 STOP)。

use std::collections::{BTreeMap, HashSet};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use api_runner::{
    run_extracts, wait_millis, Assertion, CaseOutcome, MatchCondition, Processor, ReqwestRunner,
    RequestSpec,
};

use super::local::{apply_env_static, substitute_request, CaseResultSink, CaseSpecSource};
use crate::domain::ResolvedEnv;
use crate::ports::PortError;

/// 叶子:执行一次请求。引用既有用例(取规格 + 断言 + 处理器),或内联请求。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Leaf {
    /// 引用接口用例(规格从 `CaseSpecSource` 取)。
    Case { case_id: String },
    /// 内联请求(自定义请求步骤)。`label` 仅用于结果明细的标识。
    Request {
        label: String,
        request: RequestSpec,
        assertions: Vec<Assertion>,
        processors: Vec<Processor>,
    },
}

/// 条件控制器的判定式:取上下文变量,用操作符与期望值比较(对齐前端 IfController)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Condition {
    pub variable: String,
    pub condition: MatchCondition,
    pub value: String,
}

impl Condition {
    /// 变量缺失按空串参与比较。
    pub fn eval(&self, vars: &BTreeMap<String, String>) -> bool {
        let actual = vars.get(&self.variable).map(String::as_str).unwrap_or("");
        self.condition.matches(actual, &self.value)
    }
}

/// 计划树节点。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanNode {
    Leaf(Leaf),
    /// 定次循环:body 重复 times 次。
    Loop { times: u32, body: Vec<PlanNode> },
    /// 条件分支:condition 成立才执行 body。
    If { condition: Condition, body: Vec<PlanNode> },
    /// 仅一次:同一 id 全程只进一次(被外层循环包裹也只跑首次)。
    Once { id: u32, body: Vec<PlanNode> },
    /// 等待:sleep 毫秒。
    Timer { ms: u64 },
}

/// 走树时的可变状态:上下文变量 + 是否已停 + 已执行的 Once id 集合。
struct RunState {
    vars: BTreeMap<String, String>,
    stopped: bool,
    once_done: HashSet<u32>,
}

/// 计划树执行器。
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

    /// 执行整棵计划。`stop_on_failure` 对应失败策略 STOP;上下文变量以环境变量为种子。
    /// 返回是否全部通过(任一叶子 ERROR 即 false)。
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

    /// 顺序执行一串节点。遇停止标记则中断剩余。
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
                    // insert 返回 true 表示首次见到该 id。
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
        // 取规格:CASE 从仓储取;REQUEST 用内联。取不到 → 记 ERROR。
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
        let (report, snap) = self.runner.run_case_with_snapshot(&req, &assertions).await;
        let (outcome, failures): (&str, Vec<String>) = match report.outcome {
            CaseOutcome::Success => ("SUCCESS", Vec::new()),
            CaseOutcome::Error => ("ERROR", report.failures),
        };
        self.sink.record(report_id, &case_id, outcome, &failures).await?;
        // EXTRACT 后置:写入上下文供后续节点引用。
        if let Some(s) = &snap {
            for (k, v) in run_extracts(&processors, s) {
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

    // ---- 测试替身 ----
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
        rows: Arc<Mutex<Vec<(String, String)>>>, // (case_id, outcome)
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
            self.rows.lock().expect("lock").push((case_id.to_string(), outcome.to_string()));
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
        // 缺失变量按空串
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
        assert_eq!(sink.rows.lock().expect("lock").len(), 3); // 循环 3 次
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
        // 条件成立 → 跑;不成立的分支不跑
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
        assert_eq!(sink.rows.lock().expect("lock").len(), 1); // 只跑了成立的那条
    }

    #[tokio::test]
    async fn once_inside_loop_runs_single_time() {
        let base = spawn().await;
        let specs = InMemorySpecs::default()
            .with("body", get_case(format!("{base}/ok"), vec![Assertion::StatusIs(200)], vec![]))
            .with("once", get_case(format!("{base}/ok"), vec![Assertion::StatusIs(200)], vec![]));
        let sink = SpySink::default();
        // 循环 3 次:每次跑 body,但 once 全程只跑 1 次
        let plan = vec![PlanNode::Loop {
            times: 3,
            body: vec![
                PlanNode::Leaf(Leaf::Case { case_id: "body".into() }),
                PlanNode::Once { id: 1, body: vec![PlanNode::Leaf(Leaf::Case { case_id: "once".into() })] },
            ],
        }];
        exec(specs, sink.clone()).run("r1", &plan, &ResolvedEnv::default(), false).await.expect("ok");
        let rows = sink.rows.lock().expect("lock");
        assert_eq!(rows.iter().filter(|(id, _)| id == "body").count(), 3);
        assert_eq!(rows.iter().filter(|(id, _)| id == "once").count(), 1); // 仅一次
    }

    #[tokio::test]
    async fn stop_on_failure_halts_remaining() {
        let base = spawn().await;
        let specs = InMemorySpecs::default()
            // 第一个用例断言失败(期望 500,实际 200)
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
        let rows = sink.rows.lock().expect("lock");
        assert_eq!(rows.len(), 1); // STOP:第一个失败后不再跑 after
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
        assert!(ok); // b 用到了 a 提取的 ${tk}
    }
}
