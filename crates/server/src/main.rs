#![allow(clippy::doc_lazy_continuation)]

mod breakdown_route;
mod config;
mod design_bridge;
mod debug_send;
mod decomposition_run;
mod judge;
mod llm;
mod mcp_bus;
mod mcp_tools;
mod metrics;
mod openapi;
mod orchestration;
mod case_exec_summary;
mod import_scheduler;
mod perf_run;
mod plan_run;
mod plan_scheduler;
mod project_file;
mod planner;
mod problem;
mod ratelimit;
mod references_route;
mod report_archive_job;
mod routes;
mod scenario_run;

use std::sync::Arc;
use std::time::Duration;

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::get;
use axum::Router;
use migrate::PgPool;

use bug::application::{BugFollowersUseCase, ChangeBugStatusUseCase, CreateBugUseCase, ListBugsUseCase};
use bug::adapters::pg::PgBugRepository;
use follow::application::FollowService;
use follow::adapters::pg::PgFollowStore;
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
    PgSessionStore, PgUserDirectory, PgUserRepository, PgUserRoleQuery, PgUserRoleRepository,
};
use system_setting::application::{
    CreateUserUseCase, LoginUseCase, OidcLoginUseCase, OrganizationService, ResolveUserNamesUseCase,
    RoleService, UserRoleService, UserService,
};
use system_setting::ports::PasswordHasher as _;

async fn healthz() -> &'static str {
    "ok"
}

// 2s 短超时:PG 宕机时快速 503,不卡到连接池 acquire 超时。
async fn readyz(State(pool): State<PgPool>) -> StatusCode {
    match tokio::time::timeout(Duration::from_secs(2), migrate::ping(&pool)).await {
        Ok(true) => StatusCode::OK,
        _ => StatusCode::SERVICE_UNAVAILABLE,
    }
}

fn health_routes(pool: PgPool) -> Router {
    Router::new().route("/healthz", get(healthz)).route("/readyz", get(readyz)).with_state(pool)
}

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
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,tower_http=info")),
        )
        .init();

    let cfg = config::ServerConfig::from_env();
    let bind = cfg.bind.clone();

    let pool = migrate::connect(&cfg.db_url).await?;
    migrate::run(&pool).await?;

    if std::env::args().any(|a| a == "--migrate-only") {
        tracing::info!("migrations applied; exiting (--migrate-only)");
        return Ok(());
    }

    if let Some(mock_bind) = cfg.mock_bind.clone() {
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

    let user_repo = Arc::new(PgUserRepository::new(pool.clone()));
    let user_uc = CreateUserUseCase::new(user_repo.clone());
    let user_admin = UserService::new(user_repo);
    // 展示名解析直查 ms_user,绕开任何拦截(OIDC quirk 的修复)。
    let resolve_uc = ResolveUserNamesUseCase::new(Arc::new(PgUserDirectory::new(pool.clone())));

    let hasher = Argon2PasswordHasher;
    let creds = PgCredentialRepository::new(pool.clone());
    creds
        .upsert(
            "admin",
            "u-admin",
            &hasher.hash(&cfg.admin_pw),
            &[
                "SYSTEM_USER:READ+ADD+UPDATE+DELETE".to_string(),
                "PROJECT:READ+ADD+UPDATE+DELETE".to_string(),
                "ORGANIZATION:READ+ADD+UPDATE+DELETE".to_string(),
                "USER_ROLE:READ+ADD+UPDATE+DELETE".to_string(),
                "BUG:READ+ADD+UPDATE".to_string(),
                "TEST_PLAN:READ+ADD+EXECUTE".to_string(),
                "CASE_REVIEW:READ+REVIEW".to_string(),
                "API_DEFINITION:READ+ADD+UPDATE+DELETE".to_string(),
                "API_SCENARIO:READ+ADD+UPDATE+DELETE+EXECUTE".to_string(),
                "ENVIRONMENT:READ+ADD+UPDATE+DELETE".to_string(),
                "FUNCTIONAL_CASE:READ+ADD+UPDATE+DELETE".to_string(),
                "RUNNER:READ+ADD+EDIT+EXECUTE".to_string(),
                "PERF:READ+EXECUTE".to_string(),
                "RESOURCE_POOL:READ+ADD+UPDATE+DELETE".to_string(),
                "REQUIREMENT:READ+ADD+UPDATE+DELETE".to_string(),
                "TASK:READ+ADD+EXECUTE+UPDATE".to_string(),
                "DELIVERY:READ+EXECUTE+UPDATE".to_string(),
                "VERIFICATION:READ+ADD+UPDATE".to_string(),
                "SKILL:READ+ADD+UPDATE+DELETE".to_string(),
                "COMMENT:READ+ADD+DELETE".to_string(),
            ],
        )
        .await?;
    let sessions = Arc::new(PgSessionStore::new(pool.clone()));
    let ttl_secs = cfg.session_ttl_secs;
    let role_repo = Arc::new(PgRoleRepository::new(pool.clone()));
    let user_role_repo = Arc::new(PgUserRoleRepository::new(pool.clone()));
    let mut login_uc =
        LoginUseCase::new(Arc::new(creds), Arc::new(hasher), sessions.clone(), user_role_repo.clone())
            .with_ttl_secs(ttl_secs);
    // 可选 LDAP 目录认证(本地授权 + 外部认证):仅当 SHEPHERD_LDAP_* 配齐才启用。
    if let Some(ldap) =
        system_setting::adapters::ldap::LdapAuthenticator::from_env(|k| std::env::var(k).ok())
    {
        login_uc = login_uc.with_directory(Arc::new(ldap));
        tracing::info!("registered directory authenticator: ldap");
    }
    let creds_admin: Arc<dyn system_setting::ports::CredentialRepository> =
        Arc::new(PgCredentialRepository::new(pool.clone()));
    let hasher_admin: Arc<dyn system_setting::ports::PasswordHasher> = Arc::new(Argon2PasswordHasher);
    let user_role_query: Arc<dyn system_setting::ports::UserRoleQuery> =
        Arc::new(PgUserRoleQuery::new(pool.clone()));
    let user_routes = system_setting::adapters::http::router(
        user_uc,
        resolve_uc,
        login_uc,
        user_admin,
        creds_admin,
        hasher_admin,
        user_role_query,
        sessions.clone(),
        ttl_secs,
    );

    let ext_users =
        Arc::new(PgExternalUserRepository::new(pool.clone(), vec!["PROJECT:READ".to_string()]));
    let mut oidc_uc = OidcLoginUseCase::new(ext_users, sessions.clone()).with_ttl_secs(ttl_secs);
    if let Some(p) = &cfg.feishu {
        oidc_uc = oidc_uc.register(Arc::new(FeishuProvider::new(&p.app_id, &p.app_secret, &p.redirect)));
        tracing::info!("registered OIDC provider: feishu");
    }
    if let Some(p) = &cfg.wecom {
        oidc_uc = oidc_uc.register(Arc::new(WecomProvider::new(&p.app_id, &p.app_secret, &p.redirect)));
        tracing::info!("registered OIDC provider: wecom");
    }
    let oidc_routes = system_setting::adapters::http::oidc_router(oidc_uc);

    let org_svc = OrganizationService::new(Arc::new(PgOrgRepository::new(pool.clone())));
    let org_routes = system_setting::adapters::http::org_router(org_svc, sessions.clone());

    let role_routes = system_setting::adapters::http::role_router(
        RoleService::new(role_repo.clone()),
        UserRoleService::new(role_repo, user_role_repo),
        sessions.clone(),
    );

    let project_repo = Arc::new(PgProjectRepository::new(pool.clone()));
    let project_routes = project::adapters::http::router(
        CreateProjectUseCase::new(project_repo.clone()),
        ListProjectsUseCase::new(project_repo),
        sessions.clone(),
    );
    let project_member_routes = project::adapters::member_http::router(
        project::application::ProjectMemberService::new(Arc::new(
            project::adapters::pg_member::PgProjectMemberRepository::new(pool.clone()),
        )),
        sessions.clone(),
    );

    let req_repo = Arc::new(PgRequirementRepository::new(pool.clone()));
    let req_admin = RequirementService::new(req_repo.clone());
    let requirement_routes = requirement::adapters::http::router(
        CreateRequirementUseCase::new(req_repo.clone()),
        ListRequirementsUseCase::new(req_repo.clone()),
        req_admin.clone(),
        sessions.clone(),
    );

    // 通用评论(多态挂任意实体:REQUIREMENT / BUG / FUNCTIONAL_CASE …)。
    let comment_repo = Arc::new(comment::adapters::pg::PgCommentRepository::new(pool.clone()));
    let comment_routes = comment::adapters::http::router(
        comment::application::AddCommentUseCase::new(comment_repo.clone()),
        comment::application::ListCommentsUseCase::new(comment_repo.clone()),
        comment::application::DeleteCommentUseCase::new(comment_repo),
        sessions.clone(),
    );

    let task_repo = Arc::new(PgTaskRepository::new(pool.clone()));
    let task_admin = TaskService::new(task_repo.clone());
    let task_planner = planner::build_planner();
    let task_routes = task::adapters::http::router(
        CreateDecompositionUseCase::new(task_repo.clone()),
        BreakdownUseCase::new(task_repo.clone(), task_planner.clone()),
        task_admin.clone(),
        sessions.clone(),
    );

    let ver_repo = Arc::new(PgVerificationRepository::new(pool.clone()));
    let ver_admin = VerificationService::new(ver_repo.clone());

    let case_repo = Arc::new(case_management::adapters::pg::PgCaseRepository::new(pool.clone()));
    let breakdown_routes = breakdown_route::router(
        req_admin.clone(),
        BreakdownUseCase::new(task_repo.clone(), task_planner.clone()),
        CreateVerificationUseCase::new(ver_repo.clone()),
        case_repo.clone(),
        sessions.clone(),
    );
    let verification_routes = verification::adapters::http::router(
        CreateVerificationUseCase::new(ver_repo.clone()),
        ver_admin.clone(),
        sessions.clone(),
    );

    let skill_repo = Arc::new(PgSkillRepository::new(pool.clone()));
    let skill_admin = SkillService::new(skill_repo.clone());
    let skill_routes = skill::adapters::http::router(
        CreateSkillUseCase::new(skill_repo.clone()),
        skill_admin.clone(),
        sessions.clone(),
    );

    // SHEPHERD_AGENT_ASYNC:子进程后台跑 + HTTP 自回调收尾(派发秒回,绕开 30s 请求超时)。
    let mut fleet_queue: Option<Arc<dyn delivery::ports::WorkQueue>> = None;
    let mut fleet_registry: Option<Arc<dyn delivery::ports::FleetRegistry>> = None;
    let agent: Arc<dyn AgentExecutor> = if cfg.agent.fleet {
        let redis_url = cfg.agent.fleet_redis.clone();
        let q: Arc<dyn delivery::ports::WorkQueue>;
        let reg: Arc<dyn delivery::ports::FleetRegistry>;
        if let Some(url) = &redis_url {
            q = delivery::adapters::RedisStreamQueue::connect(url, &bind)
                .await
                .expect("connect fleet redis");
            reg = delivery::adapters::RedisFleetRegistry::connect(url)
                .await
                .expect("connect fleet registry");
        } else {
            q = delivery::adapters::InMemoryWorkQueue::new();
            reg = delivery::adapters::InMemoryFleetRegistry::new();
        }
        fleet_queue = Some(q.clone());
        fleet_registry = Some(reg);
        Arc::new(delivery::adapters::QueueAgentExecutor::new(q))
    } else if let Some(url) = cfg.agent.url.clone() {
        Arc::new(delivery::adapters::agent_http::HttpAgentExecutor::new(url))
    } else if let Some(cmd) = cfg.agent.cmd.clone() {
        let argv: Vec<String> = cmd.split_whitespace().map(String::from).collect();
        let mut ex = delivery::adapters::local::LocalCommandAgentExecutor::new(argv.clone(), argv);
        if cfg.agent.async_callback {
            use webauth::SessionStore as _; // 令 sessions.create 可见
            let cb_host = bind.replace("0.0.0.0", "127.0.0.1");
            let cb_base = format!("http://{cb_host}");
            let perms = webauth::PermissionSet::from_raw(["DELIVERY:READ+UPDATE".to_string()])
                .expect("agent callback perms");
            let cb_token = sessions
                .create("agent-callback", perms, 30 * 24 * 3600)
                .await
                .expect("mint agent callback token");
            ex = ex.with_async_callback(cb_base, cb_token);
        }
        Arc::new(ex)
    } else if let Some(e) = llm::executor() {
        e
    } else {
        Arc::new(delivery::adapters::EchoAgentExecutor::new())
    };
    // 在 agent 被移入观察者前留一份克隆给 design 起草执行者。
    let design_agent = agent.clone();
    let base_delivery =
        DeliveryService::new(Arc::new(PgDeliveryRepository::new(pool.clone())), agent.clone());
    let mcp_bus = mcp_bus::McpBus::default();
    let delivery_observer = orchestration::delivery_observer(
        task_admin.clone(),
        ver_admin.clone(),
        base_delivery.clone(),
        agent,
        req_admin.clone(),
        mcp_bus.clone(),
    );
    let mut delivery_svc = base_delivery.with_observer(delivery_observer);
    if let Some(q) = &fleet_queue {
        delivery_svc = delivery_svc.with_queue(q.clone());
        // 回收任务:周期重投「持有者 runtime 已离线 + 过容忍期」的 pending(心跳判活)。
        let q = q.clone();
        let reg = fleet_registry.clone();
        let interval_s: u64 = cfg.agent.fleet_reap_interval_s;
        let grace_ms: u64 = cfg.agent.fleet_reclaim_ms;
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(std::time::Duration::from_secs(interval_s.max(1)));
            loop {
                tick.tick().await;
                let live: Vec<String> = match &reg {
                    Some(r) => {
                        r.list().await.into_iter().filter(|x| x.online).map(|x| x.id).collect()
                    }
                    None => Vec::new(),
                };
                let n = q.reclaim_dead(&live, std::time::Duration::from_millis(grace_ms)).await;
                if n > 0 {
                    tracing::warn!(requeued = n, "fleet reaper: 重投死 runtime 任务");
                }
            }
        });
    }
    let mcp_delivery = delivery_svc.clone();
    let decomposition_run_routes =
        decomposition_run::router(task_admin.clone(), delivery_svc.clone(), req_admin.clone(), sessions.clone());
    let delivery_routes = delivery::adapters::http::router(delivery_svc, sessions.clone());
    let proposal_svc = design::application::ProposalService::new(Arc::new(
        design::adapters::pg::PgProposalRepository::new(pool.clone()),
    ))
    .with_drafter(Arc::new(design::adapters::DeliveryDesignDrafter::new(
        design_agent,
        delivery::domain::ExecutorKind::ClaudeCode,
    )))
    .with_breakdown_trigger(Arc::new(design_bridge::ServerBreakdownTrigger::new(
        req_admin.clone(),
        BreakdownUseCase::new(task_repo.clone(), task_planner.clone()),
    )));
    let design_routes = design::adapters::http::router(proposal_svc, sessions.clone());
    let agent_fleet_routes = match (&fleet_queue, &fleet_registry) {
        (Some(q), Some(r)) => {
            delivery::adapters::queue::router(q.clone(), r.clone(), sessions.clone())
        }
        _ => axum::Router::new(),
    };

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
        mcp_bus,
    );

    let review_repo = Arc::new(PgReviewRepository::new(pool.clone()));
    let case_routes =
        case::adapters::http::router(SubmitReviewUseCase::new(review_repo.clone()), review_repo, sessions.clone());

    let bug_repo = Arc::new(PgBugRepository::new(pool.clone()));
    let bug_routes = bug::adapters::http::router(
        CreateBugUseCase::new(bug_repo.clone()),
        ChangeBugStatusUseCase::new(bug_repo.clone()),
        ListBugsUseCase::new(bug_repo.clone()),
        BugFollowersUseCase::new(bug_repo),
        sessions.clone(),
    );

    let follow_store = Arc::new(PgFollowStore::new(pool.clone()));
    let follow_routes =
        follow::adapters::http::router(FollowService::new(follow_store), sessions.clone());

    let plan_repo = Arc::new(PgPlanRepository::new(pool.clone()));
    let plan_routes = test_plan::adapters::http::router(
        CreatePlanUseCase::new(plan_repo.clone()),
        PlanStatisticsUseCase::new(plan_repo.clone()),
        test_plan::application::PlanCaseUseCase::new(plan_repo),
        sessions.clone(),
    );
    let plan_run_routes = plan_run::router(pool.clone(), sessions.clone());

    // 默认选 local 而非 Noop:否则 `api batch-run` 会静默停在 RUNNING 且无结果。
    let dispatcher: Arc<dyn api_test::ports::TaskDispatcher> = match &cfg.executor_url {
        Some(url) => {
            tracing::info!("api runner: HTTP dispatcher (JMeter) → {url}");
            Arc::new(HttpTaskDispatcher::new(url.clone()))
        }
        _ if cfg.runner_noop => {
            tracing::warn!("api runner: Noop(SHEPHERD_RUNNER=noop)—— 批量运行不会产出结果");
            Arc::new(api_test::adapters::NoopDispatcher)
        }
        _ => {
            tracing::info!("api runner: 本地原生 Rust runner(默认)");
            Arc::new(
                LocalRunnerDispatcher::new(
                    Arc::new(PgCaseSpecSource::new(pool.clone())),
                    Arc::new(PgCaseResultSink::new(pool.clone())),
                )
                .with_env_writer(Arc::new(api_test::adapters::pg::PgEnvironment::new(pool.clone()))),
            )
        }
    };
    let api_pools = Arc::new(PgResourcePool::new(pool.clone()));
    let api_executor = Arc::new(PgBatchReportExecutor::new(pool.clone(), dispatcher));
    let api_envs = Arc::new(api_test::adapters::pg::PgEnvironment::new(pool.clone()));
    let batch_run_uc = StartBatchRunUseCase::new(api_pools, api_executor, api_envs.clone());
    let apitest_routes = api_test::adapters::http::router(batch_run_uc.clone());
    let api_pool_admin = Arc::new(api_test::adapters::pg::PgResourcePoolAdmin::new(pool.clone()));
    let resource_pool_routes = api_test::adapters::http::resource_pool_router(
        api_test::application::CreateResourcePoolUseCase::new(api_pool_admin.clone()),
        api_test::application::ListResourcePoolsUseCase::new(api_pool_admin.clone()),
        api_test::application::EditResourcePoolUseCase::new(api_pool_admin),
        sessions.clone(),
    );
    let case_exec_routes = api_test::adapters::http::executions_router(
        api_test::application::ListCaseExecutionsUseCase::new(Arc::new(
            api_test::adapters::PgCaseExecutionQuery::new(pool.clone()),
        )),
    );

    let apidef_repo = Arc::new(api_definition::adapters::pg::PgApiDefinitionRepository::new(pool.clone()));
    let apidef_routes = api_definition::adapters::http::router(apidef_repo.clone(), sessions.clone());

    let env_repo = Arc::new(environment::adapters::pg::PgEnvironmentRepository::new(pool.clone()));
    let environment_routes = environment::adapters::http::router(env_repo, sessions.clone());

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

    let functional_case_routes = case_management::adapters::http::router(
        case_management::application::CreateCaseUseCase::new(case_repo.clone()),
        case_management::application::UpdateCaseUseCase::new(case_repo.clone()),
        case_management::application::DeleteCaseUseCase::new(case_repo.clone()),
        case_management::application::ListCasesUseCase::new(case_repo.clone()),
        case_management::application::ImportCasesUseCase::new(case_repo.clone()),
        case_repo,
        sessions.clone(),
    );

    let scenario_repo =
        Arc::new(api_scenario::adapters::pg::PgApiScenarioRepository::new(pool.clone()));
    let scenario_routes =
        api_scenario::adapters::http::router(scenario_repo.clone(), sessions.clone());
    let references_routes = references_route::router(apidef_repo.clone(), scenario_repo.clone());
    let plan_executor = api_test::adapters::plan::PlanExecutor::new(
        Arc::new(PgCaseSpecSource::new(pool.clone())),
        Arc::new(PgCaseResultSink::new(pool.clone())),
    );
    let scenario_run_routes = scenario_run::router(
        api_scenario::application::CompileScenarioUseCase::new(scenario_repo.clone()),
        plan_executor,
        api_envs.clone(),
        api_test::adapters::PgBatchReport::new(pool.clone()),
        api_scenario::application::RecordScenarioExecutionUseCase::new(scenario_repo.clone()),
        sessions.clone(),
    );

    let perf_routes = perf_run::router(
        pool.clone(),
        sessions.clone(),
        api_scenario::application::CompileScenarioUseCase::new(scenario_repo.clone()),
        scenario_repo.clone(),
        Arc::new(PgCaseSpecSource::new(pool.clone())),
        api_envs,
    );
    let debug_send_routes = debug_send::router(sessions.clone());

    let plan_scheduler_routes = plan_scheduler::build(pool.clone(), sessions.clone()).await?;

    let import_scheduler_routes = import_scheduler::build(pool.clone(), sessions.clone()).await?;

    report_archive_job::spawn(pool.clone());

    let case_summary_routes = case_exec_summary::router(pool.clone(), sessions.clone());

    let project_file_routes = project_file::router(pool.clone(), sessions.clone());

    let app = routes::assemble(vec![
        routes::group("system", user_routes.merge(oidc_routes).merge(org_routes).merge(role_routes)),
        routes::group(
            "project",
            project_routes
                .merge(project_member_routes)
                .merge(project_file_routes)
                .merge(references_routes),
        ),
        routes::group("requirement", requirement_routes.merge(comment_routes)),
        routes::group(
            "delivery",
            task_routes
                .merge(delivery_routes)
                .merge(agent_fleet_routes)
                .merge(design_routes)
                .merge(decomposition_run_routes)
                .merge(verification_routes)
                .merge(breakdown_routes),
        ),
        routes::group("skill", skill_routes.merge(mcp_routes)),
        routes::group(
            "test-case",
            case_routes.merge(case_exec_routes).merge(case_summary_routes).merge(functional_case_routes),
        ),
        routes::group("bug", bug_routes.merge(follow_routes)),
        routes::group("test-plan", plan_routes.merge(plan_run_routes).merge(plan_scheduler_routes)),
        routes::group(
            "api-test",
            apitest_routes
                .merge(resource_pool_routes)
                .merge(apidef_routes)
                .merge(environment_routes)
                .merge(runner_routes)
                .merge(scenario_routes)
                .merge(scenario_run_routes)
                .merge(import_scheduler_routes),
        ),
        routes::group("perf", perf_routes),
        routes::group("debug", debug_send_routes),
        routes::group("meta", openapi::routes().merge(health_routes(pool.clone()))),
    ]);

    let listener = tokio::net::TcpListener::bind(&bind).await?;
    tracing::info!(%bind, "server listening");
    tracing::info!(
        "routes: /healthz /readyz | /system/user | /project | /case-review | /bug | /test-plan | /api/batch-run"
    );
    axum::serve(listener, app).with_graceful_shutdown(shutdown_signal()).await?;
    Ok(())
}
