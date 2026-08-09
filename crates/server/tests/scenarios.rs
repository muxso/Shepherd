//! End-to-end business scenario tests (`#[ignore]`, need PostgreSQL). Run with:
//!   docker run -d --name ms-pg -e POSTGRES_USER=msuser -e POSTGRES_PASSWORD=mspass \
//!     -e POSTGRES_DB=mstest -p 55432:5432 postgres:16-alpine
//!   cargo test -p server --test scenarios -- --ignored --test-threads=1

use std::process::{Child, Command, Stdio};
use std::time::Duration;

use serde_json::{json, Value};

fn db_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://msuser:mspass@localhost:55432/mstest".into())
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

struct TestServer {
    child: Child,
    base: String,
    http: reqwest::Client,
}

impl Drop for TestServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl TestServer {
    async fn start() -> TestServer {
        Self::start_with_env(&[]).await
    }

    async fn start_with_env(extra: &[(&str, &str)]) -> TestServer {
        let port = free_port();
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_server"));
        cmd.env("DATABASE_URL", db_url())
            .env("SHEPHERD_BIND", format!("127.0.0.1:{port}"))
            .env("SHEPHERD_ADMIN_PASSWORD", "s3cret")
            .env("RUST_LOG", "warn");
        for (k, v) in extra {
            cmd.env(k, v);
        }
        let mut child =
            cmd.stdout(Stdio::null()).stderr(Stdio::null()).spawn().expect("spawn server binary");
        let base = format!("http://127.0.0.1:{port}");
        let http = reqwest::Client::builder().no_proxy().build().expect("client");
        for _ in 0..200 {
            if let Ok(r) = http.get(format!("{base}/healthz")).send().await {
                if r.status().is_success() {
                    return TestServer { child, base, http };
                }
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        // Reap the child on the timeout path too, or a failing test leaves an orphan server holding the port.
        let _ = child.kill();
        let _ = child.wait();
        panic!("server did not become ready within the timeout (check PostgreSQL at {})", db_url());
    }

    async fn login(&self) -> String {
        let v: Value =
            self.post("/auth/login", json!({"username":"admin","password":"s3cret"}), None).await;
        v["token"].as_str().expect("token").to_string()
    }

    async fn post(&self, path: &str, body: Value, token: Option<&str>) -> Value {
        self.send_ok("POST", path, body, token).await
    }

    async fn put(&self, path: &str, body: Value, token: Option<&str>) -> Value {
        self.send_ok("PUT", path, body, token).await
    }

    async fn send_ok(&self, method: &str, path: &str, body: Value, token: Option<&str>) -> Value {
        let url = format!("{}{}", self.base, path);
        let mut rb = match method {
            "PUT" => self.http.put(url),
            _ => self.http.post(url),
        }
        .json(&body);
        if let Some(t) = token {
            rb = rb.bearer_auth(t);
        }
        let r = rb.send().await.expect("send");
        assert!(r.status().is_success(), "{method} {path} → {}", r.status());
        r.json().await.unwrap_or(Value::Null)
    }

    async fn status(&self, method: &str, path: &str, body: Value, token: Option<&str>) -> u16 {
        let mut rb = match method {
            "POST" => self.http.post(format!("{}{}", self.base, path)).json(&body),
            _ => self.http.get(format!("{}{}", self.base, path)),
        };
        if let Some(t) = token {
            rb = rb.bearer_auth(t);
        }
        rb.send().await.expect("send").status().as_u16()
    }

    async fn get(&self, path: &str, token: &str) -> Value {
        self.http
            .get(format!("{}{}", self.base, path))
            .bearer_auth(token)
            .send()
            .await
            .expect("send")
            .json()
            .await
            .unwrap_or(Value::Null)
    }
}

/// A distinct projectId avoids uniqueness collisions in the shared database.
fn proj() -> String {
    format!("p-{}", free_port())
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn scenario_auth_and_rbac() {
    let s = TestServer::start().await;
    let p = proj();
    assert_eq!(
        s.status("POST", "/project", json!({"organizationId":&p,"name":"x"}), None).await,
        401
    );
    let t = s.login().await;
    assert_eq!(
        s.status("POST", "/project", json!({"organizationId":&p,"name":"Demo"}), Some(&t)).await,
        201
    );
    assert_eq!(
        s.status("GET", &format!("/project?organizationId={p}"), Value::Null, Some(&t)).await,
        200
    );
    let bug =
        s.post("/bug", json!({"projectId":&p,"title":"b","initialStatus":"NEW"}), Some(&t)).await;
    let bug_id = bug["id"].as_str().unwrap().to_string();
    assert_eq!(bug["status"], "NEW");
    assert_eq!(
        s.post(&format!("/bug/{bug_id}/status"), json!({"status":"RESOLVED"}), Some(&t)).await
            ["status"],
        "RESOLVED"
    );
    assert_eq!(
        s.status("POST", &format!("/bug/{bug_id}/status"), json!({"status":"NEW"}), Some(&t)).await,
        409
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn scenario_requirement_versioning() {
    let s = TestServer::start().await;
    let p = proj();
    let t = s.login().await;
    let rid = s
        .post(
            "/requirement",
            json!({"projectId":&p,"title":"login","acceptanceCriteria":["login with correct credentials"]}),
            Some(&t),
        )
        .await["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        s.post(
            &format!("/requirement/{rid}/version"),
            json!({"description":"v2","acceptanceCriteria":["supports Feishu"]}),
            Some(&t)
        )
        .await["version"],
        2
    );
    assert_eq!(s.get(&format!("/requirement/{rid}"), &t).await["baselineVersion"], 1);
    assert_eq!(
        s.get(&format!("/requirement/{rid}/version/1"), &t).await["acceptanceCriteria"][0],
        "login with correct credentials"
    );
    let v = s.put(&format!("/requirement/{rid}/baseline"), json!({"version":2}), Some(&t)).await;
    assert_eq!(v["baselineVersion"], 2);
    assert_eq!(v["status"], "BASELINED");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn scenario_shepherd_full_chain() {
    let s = TestServer::start().await;
    let p = proj();
    let t = s.login().await;
    let rid = s.post("/requirement", json!({"projectId":&p,"title":"login feature","acceptanceCriteria":["login success","wrong password rejected"]}), Some(&t)).await["id"].as_str().unwrap().to_string();
    let bd = s.post(&format!("/requirement/{rid}/breakdown"), Value::Null, Some(&t)).await;
    let did = bd["id"].as_str().unwrap().to_string();
    assert_eq!(bd["tasks"].as_array().unwrap().len(), 3);
    let vid = bd["verificationId"]
        .as_str()
        .expect("breakdown should open the verification ledger automatically")
        .to_string();
    let _ = s
        .post(
            &format!("/verification/{vid}/link"),
            json!({"criterionIndex":0,"decompositionId":&did,"taskId":"t1"}),
            Some(&t),
        )
        .await;
    let _ = s.post("/delivery", json!({"decompositionId":&did,"taskId":"t1","title":"implement login API","executor":"CLAUDE_CODE"}), Some(&t)).await;
    let dec = s.get(&format!("/decomposition/{did}"), &t).await;
    assert_eq!(dec["tasks"][0]["status"], "VERIFIED");
    let rep = s.get(&format!("/verification/{vid}/report"), &t).await;
    assert_eq!(rep["satisfied"], 1);
    assert_eq!(rep["complete"], false);
}

async fn serve_mock_llm() -> String {
    use axum::{routing::post, Json, Router};
    let app = Router::new().route(
        "/v1/chat/completions",
        post(|Json(body): Json<Value>| async move {
            let sys = body["messages"][0]["content"].as_str().unwrap_or("");
            let content = if sys.contains("规划器") {
                r#"[{"title":"LLM task A","description":"","acceptanceCriteria":["login success"],"dependencies":[]},{"title":"LLM task B","description":"","acceptanceCriteria":["wrong password rejected"],"dependencies":[0]}]"#
            } else if sys.contains("评审") {
                r#"{"passed":false,"reason":"LLM verdict: not passed (missing tests)"}"#
            } else if sys.contains("执行者") {
                r#"{"reference":"branch:llm-feat","summary":"LLM executor finished"}"#
            } else {
                "{}"
            };
            Json(json!({ "choices": [ { "message": { "content": content } } ] }))
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind llm");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move { axum::serve(listener, app).await.expect("serve llm") });
    format!("http://{addr}/v1/chat/completions")
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn scenario_llm_agent_end_to_end() {
    let llm_url = serve_mock_llm().await;
    let s = TestServer::start_with_env(&[("SHEPHERD_LLM_URL", &llm_url)]).await;
    let p = proj();
    let t = s.login().await;

    let rid = s
        .post("/requirement", json!({"projectId":&p,"title":"login feature","acceptanceCriteria":["login success","wrong password rejected"]}), Some(&t))
        .await["id"]
        .as_str()
        .unwrap()
        .to_string();

    let bd = s.post(&format!("/requirement/{rid}/breakdown"), Value::Null, Some(&t)).await;
    let did = bd["id"].as_str().unwrap().to_string();
    let tasks = bd["tasks"].as_array().unwrap();
    assert_eq!(tasks.len(), 2, "LLM planner should produce 2 tasks, not the heuristic 3: {bd}");
    assert_eq!(tasks[0]["title"], "LLM task A");

    let _ = s.post("/delivery", json!({"decompositionId":&did,"taskId":"t1","title":"implement login API","executor":"CLAUDE_CODE"}), Some(&t)).await;
    let dec = s.get(&format!("/decomposition/{did}"), &t).await;
    assert_ne!(
        dec["tasks"][0]["status"], "VERIFIED",
        "task must not be VERIFIED when the LLM verdict fails: {dec}"
    );
}

async fn serve_mock_llm_selfcorrect() -> String {
    use axum::{routing::post, Json, Router};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    let judge_calls = Arc::new(AtomicUsize::new(0));
    let app = Router::new().route(
        "/v1/chat/completions",
        post(move |Json(body): Json<Value>| {
            let jc = judge_calls.clone();
            async move {
                let sys = body["messages"][0]["content"].as_str().unwrap_or("");
                let content: String = if sys.contains("规划器") {
                    r#"[{"title":"LLM task A","description":"","acceptanceCriteria":["login success"],"dependencies":[]}]"#.into()
                } else if sys.contains("评审") {
                    // First round fails (triggers revision); subsequent rounds pass.
                    if jc.fetch_add(1, Ordering::SeqCst) == 0 {
                        r#"{"passed":false,"reason":"first round missing tests"}"#.into()
                    } else {
                        r#"{"passed":true,"reason":"meets criteria after revision"}"#.into()
                    }
                } else if sys.contains("执行者") {
                    r#"{"reference":"branch:llm","summary":"implementation/revision finished"}"#.into()
                } else {
                    "{}".into()
                };
                Json(json!({ "choices": [ { "message": { "content": content } } ] }))
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind llm");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move { axum::serve(listener, app).await.expect("serve llm") });
    format!("http://{addr}/v1/chat/completions")
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn scenario_llm_self_correction_loop() {
    let llm_url = serve_mock_llm_selfcorrect().await;
    let s = TestServer::start_with_env(&[
        ("SHEPHERD_LLM_URL", &llm_url),
        ("SHEPHERD_MAX_REVISIONS", "2"),
    ])
    .await;
    let p = proj();
    let t = s.login().await;
    let rid = s
        .post(
            "/requirement",
            json!({"projectId":&p,"title":"login","acceptanceCriteria":["login success"]}),
            Some(&t),
        )
        .await["id"]
        .as_str()
        .unwrap()
        .to_string();
    let did = s.post(&format!("/requirement/{rid}/breakdown"), Value::Null, Some(&t)).await["id"]
        .as_str()
        .unwrap()
        .to_string();
    let _ = s.post("/delivery", json!({"decompositionId":&did,"taskId":"t1","title":"implement login","executor":"CLAUDE_CODE"}), Some(&t)).await;
    let dec = s.get(&format!("/decomposition/{did}"), &t).await;
    assert_eq!(
        dec["tasks"][0]["status"], "VERIFIED",
        "should pass the verification gate after self-correction: {dec}"
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn scenario_parallel_decomposition_run() {
    let s = TestServer::start().await;
    let p = proj();
    let t = s.login().await;
    let rid = s
        .post(
            "/requirement",
            json!({"projectId":&p,"title":"parallel feature","acceptanceCriteria":["a"]}),
            Some(&t),
        )
        .await["id"]
        .as_str()
        .unwrap()
        .to_string();
    let did = s
        .post("/decomposition", json!({"requirementId":&rid,"requirementVersion":1}), Some(&t))
        .await["id"]
        .as_str()
        .unwrap()
        .to_string();
    let add = |title: &str, deps: Vec<&str>| {
        let deps: Vec<String> = deps.into_iter().map(String::from).collect();
        json!({"title":title,"acceptanceCriteria":["x"],"dependencies":deps})
    };
    s.post(&format!("/decomposition/{did}/task"), add("root", vec![]), Some(&t)).await;
    s.post(&format!("/decomposition/{did}/task"), add("left", vec!["t1"]), Some(&t)).await;
    s.post(&format!("/decomposition/{did}/task"), add("right", vec!["t1"]), Some(&t)).await;
    s.post(&format!("/decomposition/{did}/task"), add("join", vec!["t2", "t3"]), Some(&t)).await;

    let run =
        s.post(&format!("/decomposition/{did}/run"), json!({"maxConcurrency":4}), Some(&t)).await;
    assert_eq!(run["total"], 4);
    assert_eq!(run["verified"], 4, "the diamond DAG should be fully verified: {run}");
    assert_eq!(run["failed"], 0);
    assert_eq!(run["blocked"], 0);
    let rounds = run["rounds"].as_u64().unwrap();
    assert!((3..=5).contains(&rounds), "should schedule in dependency layers (≥3 rounds): {run}");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn scenario_skill_compose() {
    let s = TestServer::start().await;
    let p = proj();
    let t = s.login().await;
    let base = s
        .post(
            "/skill",
            json!({"projectId":&p,"name":"basic","instructions":"follows hexagonal architecture"}),
            Some(&t),
        )
        .await["id"]
        .as_str()
        .unwrap()
        .to_string();
    let rust = s
        .post(
            "/skill",
            json!({"projectId":&p,"name":"Rust","instructions":"using thiserror","includes":[&base]}),
            Some(&t),
        )
        .await["id"]
        .as_str()
        .unwrap()
        .to_string();
    let comp = s.post("/skill/compose", json!({"projectId":&p,"skillIds":[&rust]}), Some(&t)).await;
    assert_eq!(comp["skillIds"][0], base);
    let instr = comp["instructions"].as_str().unwrap();
    assert!(instr.contains("follows hexagonal architecture") && instr.contains("using thiserror"));
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn scenario_mcp_tools() {
    let s = TestServer::start().await;
    let t = s.login().await;
    let init = s
        .http
        .post(format!("{}/mcp", s.base))
        .bearer_auth(&t)
        .json(&json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}))
        .send()
        .await
        .expect("send");
    assert!(init.headers().contains_key("mcp-session-id"));
    let list: Value =
        s.post("/mcp", json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}), Some(&t)).await;
    let names: Vec<&str> = list["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|x| x["name"].as_str())
        .collect();
    assert!(names.contains(&"shepherd_create_requirement"));
    assert!(names.contains(&"shepherd_breakdown"));
    assert!(names.len() >= 10);
    let call: Value = s.post("/mcp", json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"shepherd_create_requirement","arguments":{"projectId":proj(),"title":"requirement created via MCP"}}}), Some(&t)).await;
    assert_eq!(call["result"]["isError"], false);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn scenario_api_definition_to_run() {
    let s = TestServer::start().await;
    let t = s.login().await;
    let org = s
        .post("/organization", json!({"name":format!("apiorg-{}", free_port())}), Some(&t))
        .await["id"]
        .as_str()
        .unwrap()
        .to_string();
    let p = s
        .post(
            "/project",
            json!({"organizationId":&org,"name":format!("apiproj-{}", free_port())}),
            Some(&t),
        )
        .await["id"]
        .as_str()
        .unwrap()
        .to_string();
    let def = s.post("/api/definition", json!({"projectId":&p,"name":"login interface","protocol":"HTTP","method":"POST","path":"/auth/login"}), Some(&t)).await;
    assert_eq!(def["status"], "DRAFT");
    let def_id = def["id"].as_str().unwrap().to_string();
    let case_id = s.post(&format!("/api/definition/{def_id}/case"), json!({"name":"correct credentials","method":"POST","url":"https://example.com/auth/login","assertions":[]}), Some(&t)).await["id"]
        .as_str().unwrap().to_string();
    assert_eq!(
        s.status(
            "POST",
            &format!("/api/definition/{def_id}/mock"),
            json!({"name":"login 200","responseStatus":200}),
            Some(&t)
        )
        .await,
        201
    );
    assert_eq!(
        s.get(&format!("/api/definition?projectId={p}"), &t).await.as_array().unwrap().len(),
        1
    );
    let scn = s.post("/api/scenario", json!({"projectId":&p,"name":"login smoke"}), Some(&t)).await
        ["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        s.status(
            "POST",
            &format!("/api/scenario/{scn}/step"),
            json!({"kind":"CASE","refMode":"REFERENCE","order":1,"refId":&case_id}),
            Some(&t)
        )
        .await,
        201
    );
    assert_eq!(s.status("POST", &format!("/api/scenario/{scn}/step"), json!({"kind":"REQUEST","order":2,"request":{"method":"GET","url":"https://example.com/health"}}), Some(&t)).await, 201);
    let comp = s.get(&format!("/api/scenario/{scn}/compile"), &t).await;
    assert_eq!(comp["steps"][0]["caseId"], case_id);
    assert_eq!(comp["steps"][1]["request"]["method"], "GET");
    assert_eq!(
        s.status(
            "POST",
            &format!("/api/scenario/{scn}/run"),
            json!({"projectId":&p,"runMode":"PARALLEL"}),
            Some(&t)
        )
        .await,
        200
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn scenario_standalone_case_and_executions() {
    let s = TestServer::start().await;
    let t = s.login().await;
    let org = s
        .post("/organization", json!({"name":format!("exorg-{}", free_port())}), Some(&t))
        .await["id"]
        .as_str()
        .unwrap()
        .to_string();
    let p = s
        .post(
            "/project",
            json!({"organizationId":&org,"name":format!("exproj-{}", free_port())}),
            Some(&t),
        )
        .await["id"]
        .as_str()
        .unwrap()
        .to_string();
    let case = s.post("/api/case", json!({"projectId":&p,"name":"standalone case","method":"GET","url":"https://example.com/ping"}), Some(&t)).await;
    assert_eq!(case["apiDefinitionId"], "");
    let case_id = case["id"].as_str().unwrap().to_string();
    let page = s.get(&format!("/api/case?projectId={p}&current=1&pageSize=10"), &t).await;
    assert_eq!(page["total"], 1);
    assert_eq!(page["totalPages"], 1);
    assert_eq!(page["items"].as_array().unwrap().len(), 1);
    let cexec = s.get(&format!("/api/case/{case_id}/executions?current=1&pageSize=10"), &t).await;
    assert_eq!(cexec["total"], 0);
    assert!(cexec["items"].is_array());
    let scn = s
        .post("/api/scenario", json!({"projectId":&p,"name":"empty execution scenario"}), Some(&t))
        .await["id"]
        .as_str()
        .unwrap()
        .to_string();
    let sexec = s.get(&format!("/api/scenario/{scn}/executions?current=1&pageSize=10"), &t).await;
    assert_eq!(sexec["total"], 0);
    assert_eq!(sexec["current"], 1);
}
