//! 组装根:把多个模块的具体实现接起来,合并路由,启动单一服务。
//!
//! 全工程只有这个文件 `use` 到了 `PgUserRepository` / `PgProjectRepository` 这类具体类型。
//! 多个业务模块共享同一个 PG 连接池;各自的 `router()` 在此 `merge` 成一个 axum 应用。
//!
//! 运行:
//!   docker run -d --name ms-pg -e POSTGRES_USER=msuser -e POSTGRES_PASSWORD=mspass \
//!     -e POSTGRES_DB=mstest -p 55432:5432 postgres:16-alpine
//!   DATABASE_URL=postgres://msuser:mspass@localhost:55432/mstest cargo run -p server

mod breakdown_route;
mod decomposition_run;
mod judge;
mod llm;
mod mcp_tools;
mod openapi;
mod orchestration;
mod perf_run;
mod plan_run;
mod plan_scheduler;
mod planner;
mod scenario_run;

use std::sync::Arc;
use std::time::Duration;

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::get;
use axum::Router;
use migrate::PgPool;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;

use bug::application::{ChangeBugStatusUseCase, CreateBugUseCase};
use bug::adapters::pg::PgBugRepository;
use api_test::application::StartBatchRunUseCase;
use api_test::adapters::jmeter::HttpTaskDispatcher;
use api_test::adapters::local::LocalRunnerDispatcher;
use api_test::adapters::pg::{
    PgBatchReportExecutor, PgCaseResultSink, PgCaseSpecSource, PgResourcePool,
};
use test_plan::application::{CreatePlanUseCase, PlanStatisticsUseCase};
use test_plan::adapters::pg::PgPlanRepository;
use case::application::SubmitReviewUseCase;
use case::adapters::pg::PgReviewRepository;
use project::application::{CreateProjectUseCase, ListProjectsUseCase};
use project::adapters::pg::PgProjectRepository;
use requirement::application::{
    CreateRequirementUseCase, ListRequirementsUseCase, RequirementService,
};
use requirement::adapters::pg::PgRequirementRepository;
use task::application::{BreakdownUseCase, CreateDecompositionUseCase, TaskService};
use task::adapters::pg::PgTaskRepository;
use delivery::application::DeliveryService;
use delivery::adapters::pg::PgDeliveryRepository;
use delivery::ports::AgentExecutor;
use verification::application::{CreateVerificationUseCase, VerificationService};
use verification::adapters::pg::PgVerificationRepository;
use skill::application::{CreateSkillUseCase, SkillService};
use skill::adapters::pg::PgSkillRepository;
use system_setting::adapters::auth::Argon2PasswordHasher;
use system_setting::adapters::oidc::{FeishuProvider, WecomProvider};
use system_setting::adapters::pg::{
    PgCredentialRepository, PgExternalUserRepository, PgOrgRepository, PgRoleRepository,
    PgSessionStore, PgUserDirectory, PgUserRepository, PgUserRoleRepository,
};
use system_setting::application::{
    CreateUserUseCase, LoginUseCase, OidcLoginUseCase, OrganizationService, ResolveUserNamesUseCase,
    RoleService, UserRoleService, UserService,
};
use system_setting::ports::PasswordHasher as _;

/// 存活探针:进程在跑即 200(不查依赖)。
async fn healthz() -> &'static str {
    "ok"
}

/// 就绪探针:能连通 PG 才 200,否则 503(供 k8s readinessProbe / LB 摘流)。
/// 给 ping 套 2s 短超时:PG 宕机时**快速** 503,而不是卡到连接池 acquire 超时。
async fn readyz(State(pool): State<PgPool>) -> StatusCode {
    match tokio::time::timeout(Duration::from_secs(2), migrate::ping(&pool)).await {
        Ok(true) => StatusCode::OK,
        _ => StatusCode::SERVICE_UNAVAILABLE,
    }
}

fn health_routes(pool: PgPool) -> Router {
    Router::new().route("/healthz", get(healthz)).route("/readyz", get(readyz)).with_state(pool)
}

/// 等待 Ctrl-C 或 SIGTERM,用于优雅关闭。
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c().await.expect("install Ctrl-C handler");
    };
    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("install SIGTERM handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!("shutdown signal received, draining...");
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 结构化日志:RUST_LOG 控制级别,默认 info(含 tower_http 请求日志)。
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,tower_http=info")),
        )
        .init();

    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://msuser:mspass@localhost:55432/mstest".to_string());
    // 默认端口与 shepherd CLI 默认连接端口(8088)对齐,开箱即用免配 --url。
    let bind = std::env::var("MS_BIND").unwrap_or_else(|_| "0.0.0.0:8088".to_string());

    // —— 一个连接池,多个模块共享;版本化迁移建/演进全部表(单一真源) ——
    let pool = migrate::connect(&db_url).await?;
    migrate::run(&pool).await?;

    // `--migrate-only`:只建表/演进 schema 然后退出(供数据迁移流程在 pgloader 前调用)。
    if std::env::args().any(|a| a == "--migrate-only") {
        tracing::info!("migrations applied; exiting (--migrate-only)");
        return Ok(());
    }

    // —— Mock 服务(可选,独立端口)——
    // 设了 MOCK_BIND 才起:catch-all 路由,据 ms_api_mock+ms_api_definition 匹配并回放响应。
    // 跑在独立监听上(被测系统把 base_url 指向它),不与主 API 路由冲突。
    if let Ok(mock_bind) = std::env::var("MOCK_BIND") {
        if !mock_bind.trim().is_empty() {
            let source = Arc::new(mock_runtime::adapters::PgMockRuleSource::new(pool.clone()));
            let mock_app = mock_runtime::adapters::http::router(source);
            let listener = tokio::net::TcpListener::bind(&mock_bind).await?;
            tracing::info!(%mock_bind, "mock server listening");
            tokio::spawn(async move {
                if let Err(e) = axum::serve(listener, mock_app).await {
                    tracing::error!("mock server error: {e}");
                }
            });
        }
    }

    // —— system-setting 模块 ——
    let user_repo = Arc::new(PgUserRepository::new(pool.clone()));
    let user_uc = CreateUserUseCase::new(user_repo.clone());
    let user_admin = UserService::new(user_repo);
    // 展示名解析走真实 PgUserDirectory(直查 ms_user,绕开任何拦截 —— OIDC quirk 的修复)
    let resolve_uc = ResolveUserNamesUseCase::new(Arc::new(PgUserDirectory::new(pool.clone())));

    // 鉴权:PG 凭证表 + 持久会话(跨重启存活)。启动时用 Argon2 幂等 upsert 一个 admin
    //(密码取 MS_ADMIN_PASSWORD,默认 "admin")。OIDC/LDAP/令牌过期见 ROADMAP B1。
    let hasher = Argon2PasswordHasher;
    let admin_pw = std::env::var("MS_ADMIN_PASSWORD").unwrap_or_else(|_| "admin".to_string());
    let creds = PgCredentialRepository::new(pool.clone());
    creds
        .upsert(
            "admin",
            "u-admin",
            &hasher.hash(&admin_pw),
            &[
                "SYSTEM_USER:READ+ADD+UPDATE+DELETE".to_string(),
                "PROJECT:READ+ADD".to_string(),
                "ORGANIZATION:READ+ADD+UPDATE+DELETE".to_string(),
                "USER_ROLE:READ+ADD+UPDATE+DELETE".to_string(),
                "BUG:READ+ADD+UPDATE".to_string(),
                "TEST_PLAN:READ+ADD+EXECUTE".to_string(),
                "CASE_REVIEW:READ+REVIEW".to_string(),
                "API_DEFINITION:READ+ADD+UPDATE+DELETE".to_string(),
                "API_SCENARIO:READ+ADD+UPDATE+DELETE+EXECUTE".to_string(),
                "ENVIRONMENT:READ+ADD+UPDATE+DELETE".to_string(),
                "FUNCTIONAL_CASE:READ+ADD".to_string(),
                "RUNNER:READ+ADD+EDIT+EXECUTE".to_string(),
                "PERF:READ+EXECUTE".to_string(),
                "RESOURCE_POOL:READ+ADD".to_string(),
                "REQUIREMENT:READ+ADD+UPDATE+DELETE".to_string(),
                "TASK:READ+ADD+EXECUTE+UPDATE".to_string(),
                "DELIVERY:READ+EXECUTE+UPDATE".to_string(),
                "VERIFICATION:READ+ADD+UPDATE".to_string(),
                "SKILL:READ+ADD+UPDATE+DELETE".to_string(),
            ],
        )
        .await?;
    let sessions = Arc::new(PgSessionStore::new(pool.clone()));
    let ttl_secs = std::env::var("MS_SESSION_TTL_SECS")
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(8 * 3600);
    // 角色仓储:登录算有效权限(凭证 ∪ 角色)+ 角色 CRUD/授权共用。
    let role_repo = Arc::new(PgRoleRepository::new(pool.clone()));
    let user_role_repo = Arc::new(PgUserRoleRepository::new(pool.clone()));
    let login_uc =
        LoginUseCase::new(Arc::new(creds), Arc::new(hasher), sessions.clone(), user_role_repo.clone())
            .with_ttl_secs(ttl_secs);
    let user_routes = system_setting::adapters::http::router(
        user_uc,
        resolve_uc,
        login_uc,
        user_admin,
        sessions.clone(),
    );

    // 第三方登录:按 env 注册飞书 / 企业微信(未配置则无该 provider,对应端点 404)。
    let ext_users =
        Arc::new(PgExternalUserRepository::new(pool.clone(), vec!["PROJECT:READ".to_string()]));
    let mut oidc_uc = OidcLoginUseCase::new(ext_users, sessions.clone()).with_ttl_secs(ttl_secs);
    if let (Ok(id), Ok(secret)) =
        (std::env::var("MS_FEISHU_APP_ID"), std::env::var("MS_FEISHU_APP_SECRET"))
    {
        let redirect = std::env::var("MS_FEISHU_REDIRECT").unwrap_or_default();
        oidc_uc = oidc_uc.register(Arc::new(FeishuProvider::new(&id, &secret, &redirect)));
        tracing::info!("registered OIDC provider: feishu");
    }
    if let (Ok(id), Ok(secret)) =
        (std::env::var("MS_WECOM_CORP_ID"), std::env::var("MS_WECOM_SECRET"))
    {
        let redirect = std::env::var("MS_WECOM_REDIRECT").unwrap_or_default();
        oidc_uc = oidc_uc.register(Arc::new(WecomProvider::new(&id, &secret, &redirect)));
        tracing::info!("registered OIDC provider: wecom");
    }
    let oidc_routes = system_setting::adapters::http::oidc_router(oidc_uc);

    // 组织 CRUD(RBAC)
    let org_svc = OrganizationService::new(Arc::new(PgOrgRepository::new(pool.clone())));
    let org_routes = system_setting::adapters::http::org_router(org_svc, sessions.clone());

    // 角色 CRUD + 用户-角色授权(RBAC)
    let role_routes = system_setting::adapters::http::role_router(
        RoleService::new(role_repo.clone()),
        UserRoleService::new(role_repo, user_role_repo),
        sessions.clone(),
    );

    // —— project 模块 ——
    let project_repo = Arc::new(PgProjectRepository::new(pool.clone()));
    let project_routes = project::adapters::http::router(
        CreateProjectUseCase::new(project_repo.clone()),
        ListProjectsUseCase::new(project_repo),
        sessions.clone(),
    );

    // —— requirement 模块(Shepherd 需求管理,多版本)——
    let req_repo = Arc::new(PgRequirementRepository::new(pool.clone()));
    let req_admin = RequirementService::new(req_repo.clone());
    let requirement_routes = requirement::adapters::http::router(
        CreateRequirementUseCase::new(req_repo.clone()),
        ListRequirementsUseCase::new(req_repo.clone()),
        req_admin.clone(),
        sessions.clone(),
    );

    // —— task 模块(Shepherd 任务拆分 DAG)——
    let task_repo = Arc::new(PgTaskRepository::new(pool.clone()));
    let task_admin = TaskService::new(task_repo.clone());
    // 规划器:默认启发式;设 SHEPHERD_PLANNER_URL 用远端 LLM 规划器。
    let task_planner = planner::build_planner();
    let task_routes = task::adapters::http::router(
        CreateDecompositionUseCase::new(task_repo.clone()),
        BreakdownUseCase::new(task_repo.clone(), task_planner.clone()),
        task_admin.clone(),
        sessions.clone(),
    );

    // —— verification 模块(Shepherd 完整性验证:需求↔任务↔实现 追溯 + 缺口检测)——
    let ver_repo = Arc::new(PgVerificationRepository::new(pool.clone()));
    let ver_admin = VerificationService::new(ver_repo.clone());

    // —— 服务端复合:据 requirementId 取规格自动拆分,并顺手开验证账本(幂等)——
    let breakdown_routes = breakdown_route::router(
        req_admin.clone(),
        BreakdownUseCase::new(task_repo.clone(), task_planner.clone()),
        CreateVerificationUseCase::new(ver_repo.clone()),
        sessions.clone(),
    );
    let verification_routes = verification::adapters::http::router(
        CreateVerificationUseCase::new(ver_repo.clone()),
        ver_admin.clone(),
        sessions.clone(),
    );

    // —— skill 模块(Shepherd AI Skill 编排:定义/复用/组合)——
    let skill_repo = Arc::new(PgSkillRepository::new(pool.clone()));
    let skill_admin = SkillService::new(skill_repo.clone());
    let skill_routes = skill::adapters::http::router(
        CreateSkillUseCase::new(skill_repo.clone()),
        skill_admin.clone(),
        sessions.clone(),
    );

    // —— delivery 模块(Shepherd 交付执行:任务 → AI 执行者)——
    // 执行者按环境路由:SHEPHERD_AGENT_URL → 远端 Agent API(异步);
    // SHEPHERD_AGENT_CMD → 本地 spawn(同步);都没配 → Echo 桩(无真实 agent)。
    let agent: Arc<dyn AgentExecutor> = if let Ok(url) = std::env::var("SHEPHERD_AGENT_URL") {
        Arc::new(delivery::adapters::agent_http::HttpAgentExecutor::new(url))
    } else if let Ok(cmd) = std::env::var("SHEPHERD_AGENT_CMD") {
        let argv: Vec<String> = cmd.split_whitespace().map(String::from).collect();
        Arc::new(delivery::adapters::local::LocalCommandAgentExecutor::new(argv.clone(), argv))
    } else if let Some(e) = llm::executor() {
        e // 真实 LLM 执行者(SHEPHERD_LLM_URL)
    } else {
        Arc::new(delivery::adapters::EchoAgentExecutor::new())
    };
    // 基础 DeliveryService(无观察者),既作交付主体也作编排器记审计的 recorder(避免 Arc 环)。
    let base_delivery =
        DeliveryService::new(Arc::new(PgDeliveryRepository::new(pool.clone())), agent.clone());
    // 交付落终态 → 编排器驱动任务 + 验证门 + 回灌验证 + 裁决记审计;
    // executor(=agent)供自纠正迭代(SHEPHERD_MAX_REVISIONS>0 时据反馈重做)。
    let delivery_observer = orchestration::delivery_observer(
        task_admin.clone(),
        ver_admin.clone(),
        base_delivery.clone(),
        agent,
    );
    let delivery_svc = base_delivery.with_observer(delivery_observer);
    let mcp_delivery = delivery_svc.clone();
    // 多任务并行编排:按依赖图逐层并发派发整张任务 DAG。
    let decomposition_run_routes =
        decomposition_run::router(task_admin.clone(), delivery_svc.clone(), sessions.clone());
    let delivery_routes = delivery::adapters::http::router(delivery_svc, sessions.clone());

    // —— MCP 集成(把全链路暴露为 MCP 工具,POST /mcp,JSON-RPC)——
    // MCP 也暴露测试计划 / 探测工具(独立实例,指向同一 pool)。
    let mcp_plan_repo = Arc::new(PgPlanRepository::new(pool.clone()));
    let mcp_remote_probe = Arc::new(runner::adapters::ReqwestRemoteProbe::new());
    let mcp_runner_svc = runner::application::RunnerService::new(
        Arc::new(runner::adapters::pg::PgRunnerAgentStore::new(pool.clone())),
        Arc::new(runner::adapters::ReqwestRemoteRunner::new()),
        mcp_remote_probe.clone(),
        mcp_remote_probe,
        Arc::new(runner::adapters::pg::PgExecutionStore::new(pool.clone())),
        Arc::new(runner::adapters::pg::PgCaseSpecSource::new(pool.clone())),
    );
    let mcp_routes = mcp_tools::router(
        CreateRequirementUseCase::new(req_repo.clone()),
        CreateDecompositionUseCase::new(task_repo.clone()),
        task_admin,
        mcp_delivery,
        req_admin,
        BreakdownUseCase::new(task_repo.clone(), task_planner),
        CreateVerificationUseCase::new(ver_repo.clone()),
        ver_admin,
        CreateSkillUseCase::new(skill_repo.clone()),
        skill_admin,
        CreatePlanUseCase::new(mcp_plan_repo.clone()),
        test_plan::application::PlanCaseUseCase::new(mcp_plan_repo.clone()),
        PlanStatisticsUseCase::new(mcp_plan_repo),
        plan_run::PlanRunner::new(pool.clone()),
        mcp_runner_svc,
        sessions.clone(),
    );

    // —— case 模块 ——
    let review_repo = Arc::new(PgReviewRepository::new(pool.clone()));
    let case_routes =
        case::adapters::http::router(SubmitReviewUseCase::new(review_repo), sessions.clone());

    // —— bug 模块 ——
    let bug_repo = Arc::new(PgBugRepository::new(pool.clone()));
    let bug_routes = bug::adapters::http::router(
        CreateBugUseCase::new(bug_repo.clone()),
        ChangeBugStatusUseCase::new(bug_repo),
        sessions.clone(),
    );

    // —— test-plan 模块 ——
    let plan_repo = Arc::new(PgPlanRepository::new(pool.clone()));
    let plan_routes = test_plan::adapters::http::router(
        CreatePlanUseCase::new(plan_repo.clone()),
        PlanStatisticsUseCase::new(plan_repo.clone()),
        test_plan::application::PlanCaseUseCase::new(plan_repo),
        sessions.clone(),
    );
    // 计划执行(跑挂入用例 + 自动回写结果)。
    let plan_run_routes = plan_run::router(pool.clone(), sessions.clone());

    // —— api-test 批量运行模块 ——
    // 执行器(端口背后的适配器,domain/application 不感知),优先级从高到低:
    //   MS_EXECUTOR_URL 非空 → HTTP 下发 JMeter 节点(异步,远端执行)
    //   MS_RUNNER=noop       → Noop 占位(显式声明本地无执行器:批量运行恒 RUNNING,仅供演示)
    //   默认                  → 原生 Rust runner(reqwest 就地跑 ms_api_case,无 JMeter)
    // 注:默认选 local 而非 Noop —— 否则 `api batch-run` 会静默停在 RUNNING 且无结果。
    let dispatcher: Arc<dyn api_test::ports::TaskDispatcher> = match std::env::var("MS_EXECUTOR_URL")
    {
        Ok(url) if !url.is_empty() => {
            tracing::info!("api runner: HTTP dispatcher (JMeter) → {url}");
            Arc::new(HttpTaskDispatcher::new(url))
        }
        _ if std::env::var("MS_RUNNER").as_deref() == Ok("noop") => {
            tracing::warn!("api runner: Noop(MS_RUNNER=noop)—— 批量运行不会产出结果");
            Arc::new(api_test::adapters::NoopDispatcher)
        }
        _ => {
            tracing::info!("api runner: 本地原生 Rust runner(默认)");
            Arc::new(LocalRunnerDispatcher::new(
                Arc::new(PgCaseSpecSource::new(pool.clone())),
                Arc::new(PgCaseResultSink::new(pool.clone())),
            ))
        }
    };
    let api_pools = Arc::new(PgResourcePool::new(pool.clone()));
    let api_executor = Arc::new(PgBatchReportExecutor::new(pool.clone(), dispatcher));
    let api_envs = Arc::new(api_test::adapters::pg::PgEnvironment::new(pool.clone()));
    let batch_run_uc = StartBatchRunUseCase::new(api_pools, api_executor, api_envs.clone());
    let apitest_routes = api_test::adapters::http::router(batch_run_uc.clone());
    // 资源池管理(创建/列出):补上 batch-run 所需池的入口,免手工 INSERT。
    let api_pool_admin = Arc::new(api_test::adapters::pg::PgResourcePoolAdmin::new(pool.clone()));
    let resource_pool_routes = api_test::adapters::http::resource_pool_router(
        api_test::application::CreateResourcePoolUseCase::new(api_pool_admin.clone()),
        api_test::application::ListResourcePoolsUseCase::new(api_pool_admin),
        sessions.clone(),
    );
    // 用例执行记录(分页读 ms_api_case_result)。
    let case_exec_routes = api_test::adapters::http::executions_router(
        api_test::application::ListCaseExecutionsUseCase::new(Arc::new(
            api_test::adapters::PgCaseExecutionQuery::new(pool.clone()),
        )),
    );

    // —— 接口定义模块(目录 + 用例 + Mock)——
    let apidef_repo = Arc::new(api_definition::adapters::pg::PgApiDefinitionRepository::new(pool.clone()));
    let apidef_routes = api_definition::adapters::http::router(apidef_repo, sessions.clone());

    // —— 环境模块(项目级 base_url + 默认头 + 变量)——
    let env_repo = Arc::new(environment::adapters::pg::PgEnvironmentRepository::new(pool.clone()));
    let environment_routes = environment::adapters::http::router(env_repo, sessions.clone());

    // —— runner 闭环(按环境注册 agent + 把用例派给 agent 就地执行)——
    // ReqwestRemoteProbe 同时承担「派发 /probe」与「探测 agent /protocols 能力」。
    let remote_probe = Arc::new(runner::adapters::ReqwestRemoteProbe::new());
    let runner_svc = runner::application::RunnerService::new(
        Arc::new(runner::adapters::pg::PgRunnerAgentStore::new(pool.clone())),
        Arc::new(runner::adapters::ReqwestRemoteRunner::new()),
        remote_probe.clone(),
        remote_probe,
        Arc::new(runner::adapters::pg::PgExecutionStore::new(pool.clone())),
        Arc::new(runner::adapters::pg::PgCaseSpecSource::new(pool.clone())),
    );
    let runner_routes = runner::adapters::http::router(runner_svc, sessions.clone());

    // —— 功能用例管理(CRUD + 自定义字段 + Excel 导出)——
    let case_repo = Arc::new(case_management::adapters::pg::PgCaseRepository::new(pool.clone()));
    let functional_case_routes = case_management::adapters::http::router(
        case_management::application::CreateCaseUseCase::new(case_repo.clone()),
        case_management::application::ListCasesUseCase::new(case_repo.clone()),
        case_management::application::ImportCasesUseCase::new(case_repo),
        sessions.clone(),
    );

    // —— 场景模块 + 组装根执行桥(编译 → 批量运行)——
    let scenario_repo =
        Arc::new(api_scenario::adapters::pg::PgApiScenarioRepository::new(pool.clone()));
    let scenario_routes =
        api_scenario::adapters::http::router(scenario_repo.clone(), sessions.clone());
    // 场景执行走计划树执行器(顺序 + 控制器 + 运行上下文),外层补建报告 + 记录。
    let plan_executor = api_test::adapters::plan::PlanExecutor::new(
        Arc::new(PgCaseSpecSource::new(pool.clone())),
        Arc::new(PgCaseResultSink::new(pool.clone())),
    );
    let scenario_run_routes = scenario_run::router(
        api_scenario::application::CompileScenarioUseCase::new(scenario_repo.clone()),
        plan_executor,
        api_envs,
        api_test::adapters::PgBatchReport::new(pool.clone()),
        api_scenario::application::RecordScenarioExecutionUseCase::new(scenario_repo),
        sessions.clone(),
    );

    // —— 原生压测(perf):POST /perf/run 后台施压 + GET /perf/report/{id} ——
    let perf_routes = perf_run::router(pool.clone(), sessions.clone());

    // —— 测试计划定时执行(cron 到点拍统计快照)——
    let plan_scheduler_routes = plan_scheduler::build(pool.clone(), sessions.clone()).await?;

    // —— 合并为单一应用 + 生产中间件 ——
    let app = user_routes
        .merge(oidc_routes)
        .merge(org_routes)
        .merge(role_routes)
        .merge(project_routes)
        .merge(requirement_routes)
        .merge(task_routes)
        .merge(delivery_routes)
        .merge(decomposition_run_routes)
        .merge(verification_routes)
        .merge(breakdown_routes)
        .merge(skill_routes)
        .merge(mcp_routes)
        .merge(case_routes)
        .merge(bug_routes)
        .merge(plan_routes)
        .merge(plan_run_routes)
        .merge(apitest_routes)
        .merge(resource_pool_routes)
        .merge(case_exec_routes)
        .merge(apidef_routes)
        .merge(environment_routes)
        .merge(functional_case_routes)
        .merge(runner_routes)
        .merge(scenario_routes)
        .merge(scenario_run_routes)
        .merge(perf_routes)
        .merge(plan_scheduler_routes)
        .merge(openapi::routes())
        .merge(health_routes(pool.clone()))
        // 由外到内:请求日志 → 整体超时 → 请求体上限(防超大 body 打爆内存)
        .layer(TraceLayer::new_for_http())
        .layer(TimeoutLayer::with_status_code(StatusCode::REQUEST_TIMEOUT, Duration::from_secs(30)))
        .layer(RequestBodyLimitLayer::new(2 * 1024 * 1024));

    let listener = tokio::net::TcpListener::bind(&bind).await?;
    tracing::info!(%bind, "server listening");
    tracing::info!(
        "routes: /healthz /readyz | /system/user | /project | /case-review | /bug | /test-plan | /api/batch-run"
    );
    // 优雅关闭:收到 Ctrl-C/SIGTERM 后停止收新连接、放完在途请求再退出。
    axum::serve(listener, app).with_graceful_shutdown(shutdown_signal()).await?;
    Ok(())
}
