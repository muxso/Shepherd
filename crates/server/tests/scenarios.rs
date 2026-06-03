//! 端到端业务场景测试(跨模块,打真实组装好的 server)。
//!
//! 与各 crate `adapters/http.rs` 里的**单模块** API 场景测试(tower::oneshot + 内存)互补:
//! 这里启动真实 `server` 二进制(连真库、跑迁移、播种 admin),用 HTTP 串起**跨上下文**链路。
//!
//! 需要 PostgreSQL,故 `#[ignore]`;运行:
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

/// 启动一个 server 实例(独立端口),退出时 kill。
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
        let port = free_port();
        let child = Command::new(env!("CARGO_BIN_EXE_server"))
            .env("DATABASE_URL", db_url())
            .env("MS_BIND", format!("127.0.0.1:{port}"))
            .env("MS_ADMIN_PASSWORD", "s3cret")
            .env("RUST_LOG", "warn")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn server binary");
        let base = format!("http://127.0.0.1:{port}");
        let http = reqwest::Client::builder().no_proxy().build().expect("client");
        // 等待健康(最多 ~20s)。
        for _ in 0..200 {
            if let Ok(r) = http.get(format!("{base}/healthz")).send().await {
                if r.status().is_success() {
                    return TestServer { child, base, http };
                }
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        panic!("server 未在超时内就绪(确认 PostgreSQL 在 {})", db_url());
    }

    async fn login(&self) -> String {
        let v: Value = self
            .post("/auth/login", json!({"username":"admin","password":"s3cret"}), None)
            .await;
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

/// 每个 case 用独立 projectId(共享库,避免唯一性冲突)。
fn proj() -> String {
    format!("p-{}", free_port())
}

// ============ 场景 1:鉴权 + RBAC(system-setting / project)============
#[tokio::test]
#[ignore = "需要 PostgreSQL"]
async fn scenario_auth_and_rbac() {
    let s = TestServer::start().await;
    let p = proj();
    // 无令牌建项目 → 401
    assert_eq!(s.status("POST", "/project", json!({"organizationId":&p,"name":"x"}), None).await, 401);
    let t = s.login().await;
    // admin 建项目 → 201;读列表开放 → 200
    assert_eq!(s.status("POST", "/project", json!({"organizationId":&p,"name":"Demo"}), Some(&t)).await, 201);
    assert_eq!(s.status("GET", &format!("/project?organizationId={p}"), Value::Null, Some(&t)).await, 200);
    // admin 持 BUG:ADD → 建缺陷 201;新项目未配状态流,回落默认种子流(NEW 合法初态)
    let bug = s.post("/bug", json!({"projectId":&p,"title":"b","initialStatus":"NEW"}), Some(&t)).await;
    let bug_id = bug["id"].as_str().unwrap().to_string();
    assert_eq!(bug["status"], "NEW");
    // 默认流允许 NEW→RESOLVED;禁止 NEW→CLOSED 跳转(状态机门禁 → 409)
    assert_eq!(s.post(&format!("/bug/{bug_id}/status"), json!({"status":"RESOLVED"}), Some(&t)).await["status"], "RESOLVED");
    assert_eq!(s.status("POST", &format!("/bug/{bug_id}/status"), json!({"status":"NEW"}), Some(&t)).await, 409);
}

// ============ 场景 2:需求多版本(requirement)============
#[tokio::test]
#[ignore = "需要 PostgreSQL"]
async fn scenario_requirement_versioning() {
    let s = TestServer::start().await;
    let p = proj();
    let t = s.login().await;
    let rid = s.post("/requirement", json!({"projectId":&p,"title":"登录","acceptanceCriteria":["正确凭证登录"]}), Some(&t)).await["id"].as_str().unwrap().to_string();
    // 修订 → v2;基线仍 v1
    assert_eq!(s.post(&format!("/requirement/{rid}/version"), json!({"description":"v2","acceptanceCriteria":["支持飞书"]}), Some(&t)).await["version"], 2);
    assert_eq!(s.get(&format!("/requirement/{rid}"), &t).await["baselineVersion"], 1);
    // v1 快照不可改写
    assert_eq!(s.get(&format!("/requirement/{rid}/version/1"), &t).await["acceptanceCriteria"][0], "正确凭证登录");
    // 定基 v2 → BASELINED(baseline 端点是 PUT)
    let v = s.put(&format!("/requirement/{rid}/baseline"), json!({"version":2}), Some(&t)).await;
    assert_eq!(v["baselineVersion"], 2);
    assert_eq!(v["status"], "BASELINED");
}

// ============ 场景 3:Shepherd 主链路(requirement→breakdown→task→delivery→verification)============
#[tokio::test]
#[ignore = "需要 PostgreSQL"]
async fn scenario_shepherd_full_chain() {
    let s = TestServer::start().await;
    let p = proj();
    let t = s.login().await;
    // 需求(2 验收标准)
    let rid = s.post("/requirement", json!({"projectId":&p,"title":"登录特性","acceptanceCriteria":["登录成功","错误密码拒绝"]}), Some(&t)).await["id"].as_str().unwrap().to_string();
    // 自动拆分(服务端取规格,默认启发式规划器)→ 3 任务(2 标准 + 集成)
    let bd = s.post(&format!("/requirement/{rid}/breakdown"), Value::Null, Some(&t)).await;
    let did = bd["id"].as_str().unwrap().to_string();
    assert_eq!(bd["tasks"].as_array().unwrap().len(), 3);
    // 开验证 + 把标准 0 追溯到 t1
    let vid = s.post("/verification", json!({"requirementId":&rid,"requirementVersion":1,"criteria":["登录成功","错误密码拒绝"]}), Some(&t)).await["id"].as_str().unwrap().to_string();
    let _ = s.post(&format!("/verification/{vid}/link"), json!({"criterionIndex":0,"decompositionId":&did,"taskId":"t1"}), Some(&t)).await;
    // 派发 t1(Echo 执行者同步完成)→ 编排器:任务驱动到 Verified + 回灌验证
    let _ = s.post("/delivery", json!({"decompositionId":&did,"taskId":"t1","title":"实现登录API","executor":"CLAUDE_CODE"}), Some(&t)).await;
    let dec = s.get(&format!("/decomposition/{did}"), &t).await;
    assert_eq!(dec["tasks"][0]["status"], "VERIFIED"); // 默认 AcceptAll 验证门 → 通过
    // 验证报告:标准 0 已满足(标准 1 未覆盖 → 仍有缺口)
    let rep = s.get(&format!("/verification/{vid}/report"), &t).await;
    assert_eq!(rep["satisfied"], 1);
    assert_eq!(rep["complete"], false);
}

// ============ 场景 4:AI Skill 组合(skill)============
#[tokio::test]
#[ignore = "需要 PostgreSQL"]
async fn scenario_skill_compose() {
    let s = TestServer::start().await;
    let p = proj();
    let t = s.login().await;
    let base = s.post("/skill", json!({"projectId":&p,"name":"基础","instructions":"遵循六边形"}), Some(&t)).await["id"].as_str().unwrap().to_string();
    let rust = s.post("/skill", json!({"projectId":&p,"name":"Rust","instructions":"用 thiserror","includes":[&base]}), Some(&t)).await["id"].as_str().unwrap().to_string();
    // 组合 Rust → 依赖在前(基础),去重
    let comp = s.post("/skill/compose", json!({"projectId":&p,"skillIds":[&rust]}), Some(&t)).await;
    assert_eq!(comp["skillIds"][0], base);
    let instr = comp["instructions"].as_str().unwrap();
    assert!(instr.contains("遵循六边形") && instr.contains("用 thiserror"));
}

// ============ 场景 5:MCP 全链路(mcp,经 JSON-RPC 工具)============
#[tokio::test]
#[ignore = "需要 PostgreSQL"]
async fn scenario_mcp_tools() {
    let s = TestServer::start().await;
    let t = s.login().await;
    // initialize → 签发 Mcp-Session-Id
    let init = s
        .http
        .post(format!("{}/mcp", s.base))
        .bearer_auth(&t)
        .json(&json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}))
        .send()
        .await
        .expect("send");
    assert!(init.headers().contains_key("mcp-session-id"));
    // tools/list 含 10 个 shepherd_ 工具
    let list: Value = s.post("/mcp", json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}), Some(&t)).await;
    let names: Vec<&str> = list["result"]["tools"].as_array().unwrap().iter().filter_map(|x| x["name"].as_str()).collect();
    assert!(names.contains(&"shepherd_create_requirement"));
    assert!(names.contains(&"shepherd_breakdown"));
    assert!(names.len() >= 10);
    // 调一个工具:建需求
    let call: Value = s.post("/mcp", json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"shepherd_create_requirement","arguments":{"projectId":proj(),"title":"经 MCP 建的需求"}}}), Some(&t)).await;
    assert_eq!(call["result"]["isError"], false);
}

// ============ 场景 6:接口测试链路(组织→项目→接口定义→用例/Mock→场景→编译→运行)============
#[tokio::test]
#[ignore = "需要 PostgreSQL"]
async fn scenario_api_definition_to_run() {
    let s = TestServer::start().await;
    let t = s.login().await;
    // 组织 → 项目
    let org = s.post("/organization", json!({"name":format!("apiorg-{}", free_port())}), Some(&t)).await["id"]
        .as_str().unwrap().to_string();
    let p = s.post("/project", json!({"organizationId":&org,"name":format!("apiproj-{}", free_port())}), Some(&t)).await["id"]
        .as_str().unwrap().to_string();
    // 接口定义
    let def = s.post("/api/definition", json!({"projectId":&p,"name":"登录接口","protocol":"HTTP","method":"POST","path":"/auth/login"}), Some(&t)).await;
    assert_eq!(def["status"], "DRAFT");
    let def_id = def["id"].as_str().unwrap().to_string();
    // 接口用例(落 ms_api_case,可被批量运行)
    let case_id = s.post(&format!("/api/definition/{def_id}/case"), json!({"name":"正确凭证","method":"POST","url":"https://example.com/auth/login","assertions":[]}), Some(&t)).await["id"]
        .as_str().unwrap().to_string();
    // Mock
    assert_eq!(s.status("POST", &format!("/api/definition/{def_id}/mock"), json!({"name":"登录200","responseStatus":200}), Some(&t)).await, 201);
    // 定义列表含 1 条
    assert_eq!(s.get(&format!("/api/definition?projectId={p}"), &t).await.as_array().unwrap().len(), 1);
    // 场景 + 两步骤(引用用例 + 内联请求)
    let scn = s.post("/api/scenario", json!({"projectId":&p,"name":"登录冒烟"}), Some(&t)).await["id"].as_str().unwrap().to_string();
    assert_eq!(s.status("POST", &format!("/api/scenario/{scn}/step"), json!({"kind":"CASE","refMode":"REFERENCE","order":1,"refId":&case_id}), Some(&t)).await, 201);
    assert_eq!(s.status("POST", &format!("/api/scenario/{scn}/step"), json!({"kind":"REQUEST","order":2,"request":{"method":"GET","url":"https://example.com/health"}}), Some(&t)).await, 201);
    // 编译 → 第一步是引用的用例 caseId,第二步是内联请求
    let comp = s.get(&format!("/api/scenario/{scn}/compile"), &t).await;
    assert_eq!(comp["steps"][0]["caseId"], case_id);
    assert_eq!(comp["steps"][1]["request"]["method"], "GET");
    // 运行:无资源池 → 桥接到批量运行用例,在入口明确 400(证明已通到池解析规则)
    assert_eq!(s.status("POST", &format!("/api/scenario/{scn}/run"), json!({"projectId":&p,"runMode":"PARALLEL"}), Some(&t)).await, 400);
}
