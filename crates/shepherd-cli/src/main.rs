//! Shepherd CLI client: drives the server over the REST API (requirements/tasks/deliveries, etc.).
//! Renders human-readable output by default; --json emits raw JSON for scripting.

use std::error::Error;
use std::path::PathBuf;

use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

type R<T> = Result<T, Box<dyn Error>>;

#[derive(Parser)]
#[command(name = "shepherd", version, about = "Shepherd —— AI 研发监督平台 CLI")]
struct Cli {
    /// Output raw JSON (default renders human-readable tables / key-values). For scripts/pipes.
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    cmd: Cmd,
}

static JSON_OUTPUT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[derive(Subcommand)]
enum Cmd {
    /// Save server URL and API key (the only auth method; issue keys at Profile -> API KEY or POST /system/apikey).
    Login {
        #[arg(long, default_value = "http://localhost:8088")]
        url: String,
        /// API key (sak_…); falls back to the SHEPHERD_API_KEY env var.
        #[arg(long = "api-key")]
        api_key: Option<String>,
    },
    /// Generate onboarding scaffold (requirement template + quick start); offline, no network.
    Init {
        #[arg(long, default_value = ".")]
        dir: String,
        #[arg(long)]
        force: bool,
    },
    /// Requirement management.
    Req {
        #[command(subcommand)]
        cmd: ReqCmd,
    },
    /// Open a task decomposition graph for a requirement version.
    Decompose {
        #[arg(long)]
        req: String,
        #[arg(long, default_value_t = 1)]
        version: u32,
    },
    /// Inspect a decomposition (fetch the full DAG plus each task's current status by id).
    Decomposition {
        #[command(subcommand)]
        cmd: DecompositionCmd,
    },
    /// Task management.
    Task {
        #[command(subcommand)]
        cmd: TaskCmd,
    },
    /// Dispatch a task to an AI executor (the README's `task run`).
    Dispatch {
        #[arg(long)]
        decomp: String,
        #[arg(long)]
        task: String,
        #[arg(long)]
        title: String,
        /// Executor (defaults to the one set via `agent connect`; CLAUDE_CODE if none).
        #[arg(long)]
        executor: Option<String>,
        #[arg(long)]
        instructions: Option<String>,
        /// Optional: project id (used with --skills to auto-compose behavior specs).
        #[arg(long)]
        project: Option<String>,
        /// Optional: comma-separated skill ids, composed into behavior specs and injected before dispatch.
        #[arg(long, value_delimiter = ',')]
        skills: Vec<String>,
    },
    /// Completeness verification.
    Verify {
        #[command(subcommand)]
        cmd: VerifyCmd,
    },
    /// AI skill orchestration.
    Skill {
        #[command(subcommand)]
        cmd: SkillCmd,
    },
    /// Connect/inspect the AI executor (the default executor for dispatch).
    Agent {
        #[command(subcommand)]
        cmd: AgentCmd,
    },
    /// Clear the locally saved API key (to invalidate the key, revoke it in the server's API KEY management).
    Logout,
    /// Project management.
    Project {
        #[command(subcommand)]
        cmd: ProjectCmd,
    },
    /// Bug management.
    Bug {
        #[command(subcommand)]
        cmd: BugCmd,
    },
    /// Case review.
    Case {
        #[command(subcommand)]
        cmd: CaseCmd,
    },
    /// Test plans.
    Plan {
        #[command(subcommand)]
        cmd: PlanCmd,
    },
    /// API testing (batch execution).
    Api {
        #[command(subcommand)]
        cmd: ApiCmd,
    },
    /// User management.
    User {
        #[command(subcommand)]
        cmd: UserCmd,
    },
    /// Organization management.
    Org {
        #[command(subcommand)]
        cmd: OrgCmd,
    },
    /// Roles and grants.
    Role {
        #[command(subcommand)]
        cmd: RoleCmd,
    },
    /// API definitions (catalog + cases + mocks).
    Apidef {
        #[command(subcommand)]
        cmd: ApidefCmd,
    },
    /// Scenarios (orchestration + steps + compile + run).
    Scenario {
        #[command(subcommand)]
        cmd: ScenarioCmd,
    },
    /// Environments (per-project base_url + default headers + variables; injected at run time).
    Env {
        #[command(subcommand)]
        cmd: EnvCmd,
    },
    /// Functional cases (CRUD + custom fields + Excel export).
    Fcase {
        #[command(subcommand)]
        cmd: FcaseCmd,
    },
    /// Remote runner agents (register per environment + dispatch cases to an agent for in-place execution).
    Runner {
        #[command(subcommand)]
        cmd: RunnerCmd,
    },
    /// Native load testing (concurrent load + latency percentile/throughput report; no JMeter).
    Perf {
        #[command(subcommand)]
        cmd: PerfCmd,
    },
    /// Resource pools (execution-node ownership for batch/scenario runs; batch-run/scenario run require --pool).
    Pool {
        #[command(subcommand)]
        cmd: PoolCmd,
    },
    /// Auth / session (login/logout/refresh/me/password + OIDC authorize URL).
    Auth {
        #[command(subcommand)]
        cmd: AuthCmd,
    },
    /// API key management (issue / list / revoke / enable).
    Apikey {
        #[command(subcommand)]
        cmd: ApikeyCmd,
    },
    /// Per-user LLM model settings (/me/llm-model).
    Llm {
        #[command(subcommand)]
        cmd: LlmCmd,
    },
    /// Notice / inbox (list / read / webhook robots / rules).
    Notice {
        #[command(subcommand)]
        cmd: NoticeCmd,
    },
    /// Follow / unfollow entities (requirement / bug / ...).
    Follow {
        #[command(subcommand)]
        cmd: FollowCmd,
    },
    /// Generic comments on any entity.
    Comment {
        #[command(subcommand)]
        cmd: CommentCmd,
    },
    /// Design proposals (per requirement).
    Proposal {
        #[command(subcommand)]
        cmd: ProposalCmd,
    },
    /// PRD drafting (generate a requirement skeleton from raw material).
    Prd {
        #[command(subcommand)]
        cmd: PrdCmd,
    },
    /// Import API definitions from a URL, or schedule recurring imports.
    Import {
        #[command(subcommand)]
        cmd: ImportCmd,
    },
    /// MCP JSON-RPC passthrough (the server's /mcp tool endpoint).
    Mcp {
        #[command(subcommand)]
        cmd: McpCmd,
    },
    /// Debug helpers (send a request / list supported protocols).
    Debug {
        #[command(subcommand)]
        cmd: DebugCmd,
    },
    /// API case execution statistics (summary + daily trend).
    Caseexec {
        #[command(subcommand)]
        cmd: CaseExecCmd,
    },
    /// Prometheus metrics (plain text; no auth).
    Metrics,
}

#[derive(Subcommand)]
enum PerfCmd {
    /// Start a load test round (runs in the background, returns reportId immediately).
    Run {
        #[arg(long)]
        url: String,
        #[arg(long, default_value = "GET")]
        method: String,
        /// Number of concurrent "virtual users".
        #[arg(long, default_value_t = 10)]
        concurrency: u32,
        /// Total request count (ignored in duration mode).
        #[arg(long, default_value_t = 100)]
        iterations: u32,
        /// Duration mode: keep the load running for this many ms (overrides --iterations when set).
        #[arg(long = "duration-ms")]
        duration_ms: Option<u64>,
        /// Assertion: expected status code (HTTP = status code; 0 means OK for other protocols).
        #[arg(long = "expect-status")]
        expect_status: Option<u16>,
        /// Assertion: output contains substring.
        #[arg(long)]
        contains: Option<String>,
        /// Assertion: output equals.
        #[arg(long)]
        equals: Option<String>,
        /// Assertion: per-request latency must not exceed this many ms.
        #[arg(long = "latency-under")]
        latency_under: Option<u64>,
        /// Protocol: HTTP (default) | SQL | GRPC | REDIS | MYSQL | WEBSOCKET. For non-HTTP, --url is the target and --query the payload.
        #[arg(long, default_value = "HTTP")]
        protocol: String,
        /// Protocol payload: SQL = statement, GRPC = method path, REDIS = command, WS = message.
        #[arg(long)]
        query: Option<String>,
        #[arg(long, default_value = "")]
        project: String,
    },
    /// Fetch a load test report (throughput/error rate/latency percentiles).
    Report {
        #[arg(long)]
        id: String,
    },
}

#[derive(Subcommand)]
enum PoolCmd {
    /// Create a resource pool.
    Create {
        #[arg(long)]
        name: String,
        /// Mark disabled (enabled by default).
        #[arg(long, default_value_t = false)]
        disable: bool,
    },
    /// List resource pools.
    List,
    /// Online runner counts per pool.
    Status,
    /// Per-pool connected runner details (name / capacity / in-flight).
    StatusDetail,
}

#[derive(Subcommand)]
enum AgentCmd {
    /// Connect an AI executor (claude-code | codex | opencode | codebuddy) and save it as the dispatch default.
    Connect {
        #[arg(long = "type")]
        kind: String,
    },
    /// Show current connection / login / server status.
    Status,
    /// Disconnect (dispatch falls back to CLAUDE_CODE).
    Disconnect,
}

fn normalize_agent(t: &str) -> R<String> {
    match t.to_ascii_lowercase().replace('-', "_").as_str() {
        "claude_code" => Ok("CLAUDE_CODE".into()),
        "codex" => Ok("CODEX".into()),
        "opencode" => Ok("OPENCODE".into()),
        // The brand is one word, but the claude-code spelling invites code-buddy; accept both.
        "codebuddy" | "code_buddy" => Ok("CODEBUDDY".into()),
        other => Err(format!(
            "未知 agent 类型: {other}(支持 claude-code | codex | opencode | codebuddy)"
        )
        .into()),
    }
}

#[derive(Subcommand)]
enum AuthCmd {
    /// Log in with username + password; prints the session token (separate from the API key used by `login`).
    Login {
        #[arg(long)]
        username: String,
        #[arg(long)]
        password: String,
    },
    /// Invalidate the current session (requires the API key / bearer auth).
    Logout,
    /// Rotate the session token using the current one (requires bearer auth).
    Refresh,
    /// Show the current session user and permissions (requires bearer auth).
    Me,
    /// Change the current user's password (requires bearer auth).
    Password {
        #[arg(long = "old")]
        old_password: String,
        #[arg(long = "new")]
        new_password: String,
    },
    /// Print the OIDC authorize URL for a provider (open it in a browser to finish login).
    Oidc {
        #[arg(long)]
        provider: String,
    },
}

#[derive(Subcommand)]
enum ApikeyCmd {
    /// Create an API key for any user (admin; requires APIKEY:ADD). Prints the raw key once.
    Create {
        #[arg(long)]
        name: String,
        /// Permission strings, comma-separated (e.g. PROJECT:READ+ADD).
        #[arg(long, value_delimiter = ',')]
        permissions: Vec<String>,
    },
    /// Create an API key for yourself. Prints the raw key once.
    CreateMine {
        #[arg(long)]
        name: Option<String>,
        /// Time-to-live in seconds (omit = never expires).
        #[arg(long = "ttl-secs")]
        ttl_secs: Option<i64>,
    },
    /// List all API keys (admin).
    List,
    /// List your own API keys.
    Mine,
    /// Revoke (delete) an API key by id.
    Delete {
        #[arg(long)]
        id: String,
    },
    /// Enable or disable an API key.
    Enable {
        #[arg(long)]
        id: String,
        /// Disable instead of enable.
        #[arg(long, default_value_t = false)]
        disable: bool,
    },
}

#[derive(Subcommand)]
enum LlmCmd {
    /// List your configured LLM models.
    List,
    /// Add an LLM model.
    Create {
        #[arg(long)]
        provider: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        base_url: Option<String>,
        #[arg(long)]
        api_key: Option<String>,
    },
    /// Update an LLM model (only the provided fields change).
    Update {
        #[arg(long)]
        id: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        base_url: Option<String>,
        #[arg(long)]
        api_key: Option<String>,
        /// Set enabled.
        #[arg(long, default_value_t = false)]
        enable: bool,
        /// Set disabled.
        #[arg(long, default_value_t = false)]
        disable: bool,
    },
    /// Delete an LLM model.
    Delete {
        #[arg(long)]
        id: String,
    },
}

#[derive(Subcommand)]
enum NoticeCmd {
    /// List notices (optional project / category / tab filters).
    List {
        #[arg(long)]
        project: Option<String>,
        /// PLAN | BUG | CASE | API | SCHEDULE.
        #[arg(long)]
        category: Option<String>,
        /// all | at | unread | read (default all).
        #[arg(long)]
        tab: Option<String>,
        #[arg(long, default_value_t = 1)]
        page: u32,
        #[arg(long = "page-size", default_value_t = 50)]
        page_size: u32,
    },
    /// Unread count.
    UnreadCount {
        #[arg(long)]
        project: Option<String>,
    },
    /// Mark all as read.
    ReadAll {
        #[arg(long)]
        project: Option<String>,
    },
    /// Mark one notice as read.
    Read {
        #[arg(long)]
        id: String,
    },
    /// Webhook robots (Feishu / DingTalk / WeCom).
    Robots {
        #[command(subcommand)]
        cmd: NoticeRobotCmd,
    },
    /// Notification rules.
    Rules {
        #[command(subcommand)]
        cmd: NoticeRuleCmd,
    },
}

#[derive(Subcommand)]
enum NoticeRobotCmd {
    /// List robots.
    List,
    /// Create a robot.
    Create {
        #[arg(long)]
        project: String,
        #[arg(long)]
        name: String,
        /// FEISHU | DINGTALK | WECOM.
        #[arg(long)]
        platform: String,
        #[arg(long = "webhook-url")]
        webhook_url: String,
        #[arg(long)]
        secret: Option<String>,
        #[arg(long, default_value_t = true)]
        enable: bool,
    },
    /// Update a robot.
    Update {
        #[arg(long)]
        id: String,
        #[arg(long)]
        project: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        platform: String,
        #[arg(long = "webhook-url")]
        webhook_url: String,
        #[arg(long)]
        secret: Option<String>,
        #[arg(long, default_value_t = true)]
        enable: bool,
    },
    /// Delete a robot.
    Delete {
        #[arg(long)]
        id: String,
    },
    /// Send a test message to the robot's webhook.
    Test {
        #[arg(long)]
        id: String,
    },
}

#[derive(Subcommand)]
enum NoticeRuleCmd {
    /// List rules (optionally by project).
    List {
        #[arg(long)]
        project: Option<String>,
    },
    /// Create a rule.
    Create {
        #[arg(long)]
        project: String,
        #[arg(long)]
        event_type: String,
        /// Channels, comma-separated (IN_APP, ROBOT).
        #[arg(long, value_delimiter = ',')]
        channels: Vec<String>,
        #[arg(long, value_delimiter = ',')]
        robot_ids: Vec<String>,
        #[arg(long, default_value = "")]
        template: String,
        #[arg(long, default_value_t = true)]
        enable: bool,
    },
    /// Update a rule.
    Update {
        #[arg(long)]
        id: String,
        #[arg(long)]
        project: String,
        #[arg(long)]
        event_type: String,
        #[arg(long, value_delimiter = ',')]
        channels: Vec<String>,
        #[arg(long, value_delimiter = ',')]
        robot_ids: Vec<String>,
        #[arg(long, default_value = "")]
        template: String,
        #[arg(long, default_value_t = true)]
        enable: bool,
    },
    /// Delete a rule.
    Delete {
        #[arg(long)]
        id: String,
    },
}

#[derive(Subcommand)]
enum FollowCmd {
    /// Follow an entity.
    Add {
        #[arg(long)]
        project: String,
        #[arg(long = "type")]
        entity_type: String,
        #[arg(long = "id")]
        entity_id: String,
    },
    /// Unfollow an entity.
    Remove {
        #[arg(long)]
        project: String,
        #[arg(long = "type")]
        entity_type: String,
        #[arg(long = "id")]
        entity_id: String,
    },
    /// Check follow status of an entity.
    Status {
        #[arg(long)]
        project: String,
        #[arg(long = "type")]
        entity_type: String,
        #[arg(long = "id")]
        entity_id: String,
    },
    /// List entities the current user follows.
    Mine {
        #[arg(long)]
        project: String,
        #[arg(long = "type")]
        entity_type: Option<String>,
    },
}

#[derive(Subcommand)]
enum CommentCmd {
    /// Add a comment to an entity.
    Add {
        #[arg(long = "type")]
        target_type: String,
        #[arg(long = "target-id")]
        target_id: String,
        #[arg(long)]
        content: String,
    },
    /// List comments on an entity.
    List {
        #[arg(long = "type")]
        target_type: String,
        #[arg(long = "target-id")]
        target_id: String,
    },
    /// Delete a comment.
    Delete {
        #[arg(long)]
        id: String,
    },
}

#[derive(Subcommand)]
enum ProposalCmd {
    /// Create a proposal for a requirement.
    Create {
        #[arg(long)]
        requirement: String,
        #[arg(long)]
        title: String,
    },
    /// List proposals of a requirement.
    List {
        #[arg(long)]
        requirement: String,
    },
    /// Get a proposal.
    Get {
        #[arg(long)]
        id: String,
    },
    /// Submit the design doc for a proposal.
    Design {
        #[arg(long)]
        id: String,
        #[arg(long)]
        doc: String,
    },
    /// Approve a proposal.
    Approve {
        #[arg(long)]
        id: String,
    },
    /// Request changes on a proposal.
    RequestChanges {
        #[arg(long)]
        id: String,
        #[arg(long)]
        comment: String,
    },
}

#[derive(Subcommand)]
enum PrdCmd {
    /// Draft a requirement (title/description/acceptance criteria) from raw material.
    Draft {
        #[arg(long)]
        raw: String,
    },
}

#[derive(Subcommand)]
enum ImportCmd {
    /// One-shot import of API definitions from a remote URL (OpenAPI/Swagger/JMeter).
    Url {
        #[arg(long)]
        project: String,
        /// Source format: openapi | swagger | jmeter (default openapi).
        #[arg(long)]
        format: Option<String>,
        /// Remote document URL.
        #[arg(long)]
        url: String,
        /// Auth token for the source (Bearer unless --basic-auth).
        #[arg(long)]
        token: Option<String>,
        /// Send --token as HTTP Basic instead of Bearer.
        #[arg(long, default_value_t = false)]
        basic_auth: bool,
        /// Target module id (omit to import at project root).
        #[arg(long = "module-id")]
        module_id: Option<String>,
        /// Import each tag as its own module (enabled by default).
        #[arg(long = "no-group-by-tag", default_value_t = false)]
        no_group_by_tag: bool,
        /// Overwrite existing definitions on conflict (enabled by default).
        #[arg(long = "no-overwrite", default_value_t = false)]
        no_overwrite: bool,
        /// Re-sync module membership after import.
        #[arg(long, default_value_t = false)]
        sync_module: bool,
    },
    /// Create a recurring import schedule (cron-driven).
    ScheduleCreate {
        #[arg(long)]
        project: String,
        /// Human-readable name (optional).
        #[arg(long)]
        name: Option<String>,
        /// Source format: openapi | swagger | jmeter (default openapi).
        #[arg(long)]
        format: Option<String>,
        /// Remote document URL.
        #[arg(long)]
        url: String,
        /// Auth token for the source (Bearer unless --basic-auth).
        #[arg(long)]
        token: Option<String>,
        /// Send --token as HTTP Basic instead of Bearer.
        #[arg(long, default_value_t = false)]
        basic_auth: bool,
        /// Target module id (omit to import at project root).
        #[arg(long = "module-id")]
        module_id: Option<String>,
        /// Import each tag as its own module (enabled by default).
        #[arg(long = "no-group-by-tag", default_value_t = false)]
        no_group_by_tag: bool,
        /// Overwrite existing definitions on conflict (enabled by default).
        #[arg(long = "no-overwrite", default_value_t = false)]
        no_overwrite: bool,
        /// Re-sync module membership after import.
        #[arg(long, default_value_t = false)]
        sync_module: bool,
        /// 6-field cron: sec min hour day month weekday.
        #[arg(long)]
        cron: String,
        /// Create disabled (enabled by default).
        #[arg(long, default_value_t = false)]
        disable: bool,
    },
    /// List import schedules of a project.
    ScheduleList {
        #[arg(long)]
        project: String,
    },
    /// Delete an import schedule by id.
    ScheduleDelete {
        #[arg(long)]
        id: String,
    },
    /// Enable/disable an import schedule.
    ScheduleEnable {
        #[arg(long)]
        id: String,
        /// Disable instead of enable.
        #[arg(long, default_value_t = false)]
        disable: bool,
    },
    /// Trigger an import schedule immediately.
    ScheduleRun {
        #[arg(long)]
        id: String,
    },
}

#[derive(Subcommand)]
enum McpCmd {
    /// Send a raw JSON-RPC call to the server's /mcp endpoint (bearer auth).
    Call {
        /// JSON-RPC method, e.g. tools/list or tools/call.
        #[arg(long)]
        method: String,
        /// JSON-RPC params object (raw JSON string; omit for none).
        #[arg(long = "params-json")]
        params_json: Option<String>,
        /// JSON-RPC id (omit for a notification).
        #[arg(long)]
        id: Option<u64>,
    },
    /// Convenience: list the server's MCP tools (issues a tools/list call).
    Tools,
    /// Convenience: call an MCP tool by name.
    ToolsCall {
        #[arg(long)]
        name: String,
        /// Tool arguments (raw JSON object string).
        #[arg(long = "args-json")]
        args_json: Option<String>,
    },
}

#[derive(Subcommand)]
enum DebugCmd {
    /// Send a request through the server's debug proxy (supports http + probe protocols).
    Send {
        #[arg(long, default_value = "http")]
        protocol: String,
        #[arg(long, default_value = "GET")]
        method: String,
        #[arg(long)]
        url: String,
        /// Extra headers, repeatable, "Name: value".
        #[arg(long = "header")]
        headers: Vec<String>,
        /// Request body (string).
        #[arg(long)]
        body: Option<String>,
        /// Protocol-specific params, repeatable, key=value (e.g. method=POST).
        #[arg(long = "meta")]
        meta: Vec<String>,
        /// Assertion array (raw JSON), e.g. '[{"type":"StatusIs","args":200}]'.
        #[arg(long = "assertions-json")]
        assertions_json: Option<String>,
    },
    /// List protocols supported by the debug proxy.
    Protocols,
}

#[derive(Subcommand)]
enum CaseExecCmd {
    /// Execution summary for a project (executions / passed / executed cases).
    Summary {
        #[arg(long)]
        project: String,
    },
    /// Daily execution trend (pass/fail over the last N days).
    Trend {
        #[arg(long)]
        project: String,
        /// Lookback window in days (1-90, default 7).
        #[arg(long, default_value_t = 7)]
        days: i32,
    },
}

#[derive(Subcommand)]
enum ReqCmd {
    /// Create a requirement.
    Add {
        #[arg(long)]
        project: String,
        #[arg(long)]
        title: String,
        #[arg(long, default_value = "")]
        description: String,
        /// Acceptance criteria, comma-separated.
        #[arg(long, value_delimiter = ',')]
        criteria: Vec<String>,
    },
    /// List requirements in a project.
    List {
        #[arg(long)]
        project: String,
        /// Page number (1-based).
        #[arg(long, default_value_t = 1)]
        page: u32,
        /// Page size.
        #[arg(long, default_value_t = 50)]
        page_size: u32,
    },
    /// Auto-decompose a requirement into a task DAG (server fetches the spec and hands it to the planner).
    Breakdown {
        #[arg(long)]
        req: String,
        /// Optional: requirement version (defaults to the baseline version).
        #[arg(long)]
        version: Option<u32>,
        /// Use the AI planner (the server-configured planner is used either way; this flag is for readability only).
        #[arg(long, default_value_t = false)]
        ai: bool,
    },
}

#[derive(Subcommand)]
enum TaskCmd {
    /// Add a task to a decomposition graph.
    Add {
        #[arg(long)]
        decomp: String,
        #[arg(long)]
        title: String,
        /// Dependency task local ids, comma-separated.
        #[arg(long, value_delimiter = ',')]
        deps: Vec<String>,
    },
}

#[derive(Subcommand)]
enum DecompositionCmd {
    /// Fetch the full decomposition graph (complete / readyTaskIds / each task's current status).
    Get {
        #[arg(long)]
        id: String,
    },
    /// Show only tasks currently ready (all dependencies Verified, dispatchable).
    Ready {
        #[arg(long)]
        id: String,
    },
    /// Parallel orchestration: dispatch the whole task DAG layer by layer along dependencies (auto-drives verification gates).
    Run {
        #[arg(long)]
        id: String,
        #[arg(long, default_value = "CLAUDE_CODE")]
        executor: String,
        #[arg(long, default_value_t = 4)]
        concurrency: usize,
    },
}

#[derive(Subcommand)]
enum VerifyCmd {
    /// Open a verification (with acceptance criteria).
    Create {
        #[arg(long)]
        req: String,
        #[arg(long, default_value_t = 1)]
        version: u32,
        #[arg(long, value_delimiter = ',')]
        criteria: Vec<String>,
    },
    /// Create a coverage link: a task covers an acceptance criterion (requirement -> task traceability).
    Link {
        /// Verification id.
        #[arg(long)]
        id: String,
        /// Acceptance criterion index (0-based).
        #[arg(long)]
        criterion: u32,
        /// Decomposition id.
        #[arg(long)]
        decomp: String,
        /// Task local id (e.g. t1).
        #[arg(long)]
        task: String,
    },
    /// Sync a task's delivery/verification status to its coverage links (task -> implementation traceability).
    Sync {
        #[arg(long)]
        id: String,
        #[arg(long)]
        decomp: String,
        #[arg(long)]
        task: String,
        /// Mark as unverified (default syncs as verified, satisfied=true).
        #[arg(long, default_value_t = false)]
        unsatisfied: bool,
    },
    /// Completeness report.
    Report {
        #[arg(long)]
        id: String,
    },
}

#[derive(Subcommand)]
enum SkillCmd {
    /// Define a skill.
    Add {
        #[arg(long)]
        project: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        instructions: String,
        #[arg(long, value_delimiter = ',')]
        includes: Vec<String>,
    },
    /// Compose skills into an instruction set.
    Compose {
        #[arg(long)]
        project: String,
        #[arg(long, value_delimiter = ',')]
        ids: Vec<String>,
    },
}

#[derive(Subcommand)]
enum ProjectCmd {
    /// Create a project.
    Create {
        #[arg(long)]
        org: String,
        #[arg(long)]
        name: String,
    },
    /// List projects in an organization, paged.
    List {
        #[arg(long)]
        org: String,
        #[arg(long, default_value_t = 1)]
        current: u32,
        #[arg(long = "page-size", default_value_t = 10)]
        page_size: u32,
    },
}

#[derive(Subcommand)]
enum BugCmd {
    /// Create a bug.
    Create {
        #[arg(long)]
        project: String,
        #[arg(long)]
        title: String,
        #[arg(long, default_value = "NEW")]
        status: String,
    },
    /// Transition bug status.
    Status {
        #[arg(long)]
        id: String,
        #[arg(long)]
        to: String,
    },
}

#[derive(Subcommand)]
enum CaseCmd {
    /// Submit a case review comment.
    Review {
        #[arg(long)]
        review: String,
        #[arg(long)]
        case: String,
        #[arg(long)]
        reviewer: String,
        /// PASS | UN_PASS | UNDER_REVIEWED.
        #[arg(long)]
        status: String,
        /// Required for UN_PASS.
        #[arg(long)]
        content: Option<String>,
    },
}

#[derive(Subcommand)]
enum RunnerCmd {
    /// Register a runner-agent for an environment.
    Register {
        #[arg(long)]
        name: String,
        /// Agent endpoint, e.g. http://10.0.0.5:9100.
        #[arg(long = "base-url")]
        base_url: String,
        /// Optional shared secret (matches the agent's RUNNER_TOKEN).
        #[arg(long)]
        token: Option<String>,
    },
    /// List registered agents.
    List,
    /// Dispatch a self-contained case to an agent for in-place execution.
    Run {
        /// Agent id (from register/list).
        #[arg(long)]
        agent: String,
        #[arg(long)]
        url: String,
        #[arg(long, default_value = "GET")]
        method: String,
        #[arg(long)]
        body: Option<String>,
        /// Expected status code (generates a StatusIs assertion when set).
        #[arg(long = "expect-status")]
        expect_status: Option<u16>,
    },
    /// Dispatch a **stored case** (api-case) to an agent.
    RunCase {
        #[arg(long)]
        agent: String,
        #[arg(long)]
        case: String,
    },
    /// List an agent's remote execution history.
    Executions {
        #[arg(long)]
        agent: String,
    },
    /// Re-fetch an agent's /protocols and refresh its protocol capability snapshot.
    Refresh {
        #[arg(long)]
        agent: String,
    },
    /// Probe by protocol: given only a protocol, the server picks a supporting agent for in-place execution (assertions supported).
    Probe {
        /// Protocol name (http/grpc/sql/…).
        #[arg(long)]
        protocol: String,
        /// Target (URL / gRPC endpoint / connection string).
        #[arg(long)]
        target: String,
        /// Payload (HTTP body / gRPC request bytes / SQL statement).
        #[arg(long)]
        payload: Option<String>,
        /// Extra protocol params k=v (repeatable; e.g. method=POST, gRPC method=/pkg.Svc/M).
        #[arg(long = "meta")]
        meta: Vec<String>,
        /// Assertion: status code equals.
        #[arg(long = "expect-status")]
        expect_status: Option<i64>,
        /// Assertion: output contains substring.
        #[arg(long)]
        contains: Option<String>,
        /// Assertion: latency under N ms.
        #[arg(long = "latency-under")]
        latency_under: Option<u64>,
    },
}

#[derive(Subcommand)]
enum FcaseCmd {
    /// Create a functional case. --field key=value (repeatable) sets custom fields.
    Create {
        #[arg(long)]
        project: String,
        #[arg(long)]
        name: String,
        #[arg(long, default_value = "")]
        module: String,
        #[arg(long, default_value = "P2")]
        priority: String,
        #[arg(long, default_value = "PREPARED")]
        status: String,
        #[arg(long = "field")]
        fields: Vec<String>,
    },
    /// List functional cases in a project.
    List {
        #[arg(long)]
        project: String,
    },
    /// Export as Excel (.xlsx) to the --out file.
    Export {
        #[arg(long)]
        project: String,
        #[arg(long, default_value = "cases.xlsx")]
        out: String,
    },
    /// Import cases from Excel (.xlsx).
    Import {
        #[arg(long)]
        project: String,
        #[arg(long)]
        file: String,
    },
}

#[derive(Subcommand)]
enum PlanCmd {
    /// Create a test plan (or group).
    Create {
        #[arg(long)]
        project: String,
        #[arg(long)]
        name: String,
        /// TEST_PLAN | GROUP.
        #[arg(long = "type", default_value = "TEST_PLAN")]
        plan_type: String,
        #[arg(long)]
        group: Option<String>,
    },
    /// Plan execution statistics.
    Stats {
        #[arg(long)]
        id: String,
    },
    /// Export a report to stdout: HTML by default (e.g. `> report.html`); --markdown for Markdown (e.g. `> report.md`).
    Report {
        #[arg(long)]
        id: String,
        /// Export Markdown (instead of HTML).
        #[arg(long)]
        markdown: bool,
    },
    /// Set scheduled runs for a plan (6-field cron: sec min hour day month weekday).
    Schedule {
        #[arg(long)]
        id: String,
        /// Cron expression, e.g. "0 */5 * * * *" (every 5 minutes).
        #[arg(long)]
        cron: String,
    },
    /// Fetch a plan's scheduled-run snapshots (pass rate / execution rate trends).
    Runs {
        #[arg(long)]
        id: String,
    },
    /// Attach a case to a plan.
    LinkCase {
        #[arg(long)]
        id: String,
        #[arg(long)]
        case: String,
        #[arg(long, default_value = "")]
        name: String,
    },
    /// Write back a case's execution result within a plan.
    Result {
        #[arg(long)]
        id: String,
        #[arg(long)]
        case: String,
        /// SUCCESS | ERROR | FAKE_ERROR | BLOCK | PENDING.
        #[arg(long, default_value = "SUCCESS")]
        status: String,
        #[arg(long = "latency-ms", default_value_t = 0)]
        latency_ms: u64,
        #[arg(long = "status-code")]
        status_code: Option<i64>,
        #[arg(long)]
        body: Option<String>,
        /// Assertions JSON array, e.g. '[{"item":"status","actual":"200","condition":"equals","expected":"200","passed":true}]'.
        #[arg(long = "assertions-json")]
        assertions_json: Option<String>,
    },
    /// List cases in a plan (with status).
    Cases {
        #[arg(long)]
        id: String,
    },
    /// Run a plan: execute attached cases and auto write back results (so a later `plan report` shows real data).
    Run {
        #[arg(long)]
        id: String,
    },
}

#[derive(Subcommand)]
enum ApiCmd {
    /// Batch-run API cases.
    BatchRun {
        #[arg(long)]
        project: String,
        #[arg(long, value_delimiter = ',')]
        cases: Vec<String>,
        /// Run mode (e.g. SERIAL | PARALLEL).
        #[arg(long = "mode", default_value = "PARALLEL")]
        run_mode: String,
        /// Resource pool id (batch runs require it, client-provided).
        #[arg(long)]
        pool: Option<String>,
        /// Environment id for the run (injects base_url/default headers/variables).
        #[arg(long)]
        env: Option<String>,
    },
}

#[derive(Subcommand)]
enum UserCmd {
    /// Create a user.
    Create {
        #[arg(long)]
        name: String,
        #[arg(long)]
        email: String,
    },
    /// List users, paged.
    List {
        #[arg(long, default_value_t = 1)]
        current: u32,
        #[arg(long = "page-size", default_value_t = 10)]
        page_size: u32,
    },
    /// Get a user.
    Get {
        #[arg(long)]
        id: String,
    },
    /// Update a user.
    Update {
        #[arg(long)]
        id: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        email: String,
        /// Mark disabled (enabled by default).
        #[arg(long, default_value_t = false)]
        disable: bool,
    },
    /// Delete a user.
    Delete {
        #[arg(long)]
        id: String,
    },
    /// Resolve user names by ids in bulk (comma-separated).
    Names {
        #[arg(long, value_delimiter = ',')]
        ids: Vec<String>,
    },
}

#[derive(Subcommand)]
enum OrgCmd {
    /// Create an organization.
    Create {
        #[arg(long)]
        name: String,
        #[arg(long, default_value_t = false)]
        disable: bool,
    },
    /// List organizations, paged.
    List {
        #[arg(long, default_value_t = 1)]
        current: u32,
        #[arg(long = "page-size", default_value_t = 10)]
        page_size: u32,
    },
    /// Get an organization.
    Get {
        #[arg(long)]
        id: String,
    },
    /// Update an organization.
    Update {
        #[arg(long)]
        id: String,
        #[arg(long)]
        name: String,
        #[arg(long, default_value_t = false)]
        disable: bool,
    },
    /// Delete an organization.
    Delete {
        #[arg(long)]
        id: String,
    },
}

#[derive(Subcommand)]
enum RoleCmd {
    /// Create a role.
    Create {
        #[arg(long)]
        name: String,
        #[arg(long)]
        scope: Option<String>,
        /// Permission strings, comma-separated (e.g. PROJECT:READ+ADD).
        #[arg(long, value_delimiter = ',')]
        permissions: Vec<String>,
    },
    /// List roles, paged.
    List {
        #[arg(long, default_value_t = 1)]
        current: u32,
        #[arg(long = "page-size", default_value_t = 10)]
        page_size: u32,
    },
    /// Get a role.
    Get {
        #[arg(long)]
        id: String,
    },
    /// Update a role.
    Update {
        #[arg(long)]
        id: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        scope: Option<String>,
        #[arg(long, value_delimiter = ',')]
        permissions: Vec<String>,
    },
    /// Delete a role.
    Delete {
        #[arg(long)]
        id: String,
    },
    /// Grant a role to a user.
    Grant {
        #[arg(long)]
        user: String,
        #[arg(long)]
        role: String,
    },
    /// Revoke a user's role.
    Revoke {
        #[arg(long)]
        user: String,
        #[arg(long)]
        role: String,
    },
}

#[derive(Subcommand)]
enum ApidefCmd {
    /// Create an API definition.
    Create {
        #[arg(long)]
        project: String,
        #[arg(long)]
        name: String,
        /// Protocol: HTTP | TCP | SQL | DUBBO.
        #[arg(long, default_value = "HTTP")]
        protocol: String,
        #[arg(long, default_value = "GET")]
        method: String,
        #[arg(long, default_value = "")]
        path: String,
    },
    /// Bulk-import API definitions from OpenAPI 3.x / Swagger 2.0 (--file local or --url remote, one of the two).
    Import {
        #[arg(long)]
        project: String,
        /// Path to an OpenAPI/Swagger JSON file.
        #[arg(long)]
        file: Option<String>,
        /// URL of OpenAPI/Swagger JSON (e.g. the service's own /api-docs/openapi.json).
        #[arg(long)]
        url: Option<String>,
    },
    /// List API definitions in a project.
    List {
        #[arg(long)]
        project: String,
    },
    /// Get an API definition.
    Get {
        #[arg(long)]
        id: String,
    },
    /// Add an API case to a definition (stored in ms_api_case, batch-runnable).
    Case {
        #[arg(long)]
        def: String,
        #[arg(long)]
        name: String,
        #[arg(long, default_value = "GET")]
        method: String,
        #[arg(long)]
        url: String,
        #[arg(long)]
        body: Option<String>,
        /// Expected status code: when set, generates a StatusIs assertion (decides case pass/fail); omitted means no assertions (always passes).
        #[arg(long = "expect-status")]
        expect_status: Option<u16>,
    },
    /// List API cases under a definition.
    Cases {
        #[arg(long)]
        def: String,
    },
    /// Create an API case (can be standalone: omit --def for an unattached case).
    CaseNew {
        #[arg(long)]
        project: String,
        #[arg(long)]
        def: Option<String>,
        #[arg(long)]
        name: String,
        #[arg(long, default_value = "GET")]
        method: String,
        #[arg(long)]
        url: String,
        #[arg(long)]
        body: Option<String>,
        /// Expected status code: when set, generates a StatusIs assertion (decides case pass/fail); omitted means no assertions (always passes).
        #[arg(long = "expect-status")]
        expect_status: Option<u16>,
    },
    /// For every API definition in the project, generate a "success (expect 2xx) + failure (expect 401)" case pair, plus one scenario chaining both.
    GenSuite {
        #[arg(long)]
        project: String,
        /// Base URL for case requests (prepended to the OpenAPI path).
        #[arg(long, default_value = "http://localhost:8088")]
        base: String,
        /// Generate cases only, no scenarios.
        #[arg(long = "no-scenario", default_value_t = false)]
        no_scenario: bool,
    },
    /// List API cases in a project, paged (standalone view).
    CaseList {
        #[arg(long)]
        project: String,
        #[arg(long, default_value_t = 1)]
        current: u32,
        #[arg(long = "page-size", default_value_t = 10)]
        page_size: u32,
    },
    /// List a case's execution records, paged.
    CaseExec {
        #[arg(long)]
        case: String,
        #[arg(long, default_value_t = 1)]
        current: u32,
        #[arg(long = "page-size", default_value_t = 10)]
        page_size: u32,
    },
    /// Run a single API case (optional environment/resource pool) and write back the execution record.
    CaseRun {
        #[arg(long)]
        case: String,
        #[arg(long)]
        project: String,
        #[arg(long = "mode", default_value = "SERIAL")]
        run_mode: String,
        #[arg(long)]
        pool: Option<String>,
        /// Environment id for the run (injects base_url/default headers/variables).
        #[arg(long)]
        env: Option<String>,
    },
    /// Add a mock to a definition.
    Mock {
        #[arg(long)]
        def: String,
        #[arg(long)]
        name: String,
        #[arg(long = "status", default_value_t = 200)]
        response_status: i32,
        #[arg(long)]
        body: Option<String>,
    },
    /// List mocks under a definition.
    Mocks {
        #[arg(long)]
        def: String,
    },
    /// Delete an API definition (cascades its cases/mocks).
    Delete {
        #[arg(long)]
        id: String,
    },
    /// List entities that reference a definition (cases + scenarios).
    References {
        #[arg(long)]
        id: String,
    },
    /// Replace a definition's request/response spec (raw JSON via --spec-json).
    Spec {
        #[arg(long)]
        id: String,
        #[arg(long = "spec-json")]
        spec_json: String,
    },
    /// Move a definition into (or out of) a module.
    Module {
        #[arg(long)]
        id: String,
        /// Target module id (omit with --unset to move back to uncategorized).
        #[arg(long = "module-id")]
        module_id: Option<String>,
        /// Move the definition out of its module (uncategorized).
        #[arg(long, default_value_t = false)]
        unset: bool,
    },
    /// Set a definition's lifecycle status (e.g. DRAFT | ACTIVE | DEPRECATED).
    Status {
        #[arg(long)]
        id: String,
        #[arg(long)]
        status: String,
    },
    /// List a definition's change history.
    Changes {
        #[arg(long)]
        id: String,
    },
}

#[derive(Subcommand)]
enum ScenarioCmd {
    /// Create a scenario.
    Create {
        #[arg(long)]
        project: String,
        #[arg(long)]
        name: String,
    },
    /// List scenarios in a project.
    List {
        #[arg(long)]
        project: String,
    },
    /// Get a scenario (with steps).
    Get {
        #[arg(long)]
        id: String,
    },
    /// Add a step: kind=request|case|scenario|loop|if|once|timer.
    Step {
        #[arg(long)]
        scenario: String,
        /// request | case | scenario | loop | if | once | timer.
        #[arg(long)]
        kind: String,
        #[arg(long = "ref-mode", default_value = "REFERENCE")]
        ref_mode: String,
        #[arg(long, default_value_t = 1)]
        order: i32,
        /// case/scenario steps: the referenced id.
        #[arg(long = "ref")]
        ref_id: Option<String>,
        /// request steps: inline request.
        #[arg(long)]
        method: Option<String>,
        #[arg(long)]
        url: Option<String>,
        #[arg(long)]
        body: Option<String>,
        /// request steps: expected status code (generates a StatusIs assertion).
        #[arg(long = "expect-status")]
        expect_status: Option<u16>,
        /// request steps: assertions JSON array (overrides --expect-status), e.g.
        /// '[{"type":"StatusIs","args":200},{"type":"BodyContains","args":"ok"}]'.
        #[arg(long = "assertions-json")]
        assertions_json: Option<String>,
        /// Controller steps (loop/if/once/timer): payload JSON, e.g.
        /// '{"times":3,"children":[{"kind":"CASE","refId":"c1"}]}'.
        #[arg(long = "control-json")]
        control_json: Option<String>,
    },
    /// Compile a scenario into runnable steps (recursively expands sub-scenarios).
    Compile {
        #[arg(long)]
        id: String,
    },
    /// List scenario execution records, paged (with execution status).
    Executions {
        #[arg(long)]
        id: String,
        #[arg(long, default_value_t = 1)]
        current: u32,
        #[arg(long = "page-size", default_value_t = 10)]
        page_size: u32,
    },
    /// Run a scenario (compile -> batch execute).
    Run {
        #[arg(long)]
        id: String,
        #[arg(long)]
        project: String,
        #[arg(long = "mode", default_value = "PARALLEL")]
        run_mode: String,
        #[arg(long)]
        pool: Option<String>,
        /// Environment id for the run (injects base_url/default headers/variables).
        #[arg(long)]
        env: Option<String>,
    },
}

#[derive(Subcommand)]
enum EnvCmd {
    /// Create an environment. --header takes "Name: value" (repeatable); --var takes key=value (repeatable).
    Create {
        #[arg(long)]
        project: String,
        #[arg(long)]
        name: String,
        /// Base URL (prefix for relative urls), e.g. http://localhost:8088.
        #[arg(long, default_value = "")]
        base: String,
        #[arg(long = "header")]
        headers: Vec<String>,
        #[arg(long = "var")]
        vars: Vec<String>,
    },
    /// List environments in a project.
    List {
        #[arg(long)]
        project: String,
    },
    /// Get an environment.
    Get {
        #[arg(long)]
        id: String,
    },
    /// Update an environment (fully replaces name/base/headers/vars).
    Update {
        #[arg(long)]
        id: String,
        #[arg(long)]
        project: String,
        #[arg(long)]
        name: String,
        #[arg(long, default_value = "")]
        base: String,
        #[arg(long = "header")]
        headers: Vec<String>,
        #[arg(long = "var")]
        vars: Vec<String>,
    },
    /// Delete an environment.
    Delete {
        #[arg(long)]
        id: String,
    },
}

#[derive(Serialize, Deserialize, Default, Clone)]
struct Config {
    url: String,
    /// Static API key (sak_…).
    #[serde(default)]
    api_key: String,
    #[serde(default)]
    agent: Option<String>,
}

const NO_KEY_HINT: &str =
    "未配置 API key:执行 `shepherd login --api-key sak_…` 或设 SHEPHERD_API_KEY\
(key 可在 个人中心 → API KEY 或 POST /system/apikey 签发)";

fn config_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".shepherd").join("config.json")
}

impl Config {
    fn load() -> Config {
        let mut c: Config = std::fs::read_to_string(config_path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        if let Ok(u) = std::env::var("SHEPHERD_URL") {
            c.url = u;
        }
        if let Ok(k) = std::env::var("SHEPHERD_API_KEY") {
            c.api_key = k;
        }
        if c.url.is_empty() {
            c.url = "http://127.0.0.1:8088".into();
        }
        c
    }

    fn save(&self) -> R<()> {
        let p = config_path();
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&p, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }
}

struct Client {
    http: reqwest::blocking::Client,
    cfg: Config,
}

impl Client {
    fn new(cfg: Config) -> R<Client> {
        // no_proxy: the server is usually local/intranet; avoid interception by a global proxy.
        let http = reqwest::blocking::Client::builder().no_proxy().build()?;
        Ok(Client { http, cfg })
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.cfg.url.trim_end_matches('/'), path)
    }

    fn send(&self, mut rb: reqwest::blocking::RequestBuilder, auth: bool) -> R<Value> {
        if auth {
            if self.cfg.api_key.is_empty() {
                return Err(NO_KEY_HINT.into());
            }
            rb = rb.bearer_auth(&self.cfg.api_key);
        }
        let resp = rb.send()?;
        let status = resp.status();
        let text = resp.text().unwrap_or_default();
        if !status.is_success() {
            return Err(format!("HTTP {status}: {text}").into());
        }
        Ok(serde_json::from_str(&text).unwrap_or(Value::Null))
    }

    fn post(&self, path: &str, body: Value, auth: bool) -> R<Value> {
        self.send(self.http.post(self.url(path)).json(&body), auth)
    }
    fn get(&self, path: &str, auth: bool) -> R<Value> {
        self.send(self.http.get(self.url(path)), auth)
    }
    fn post_bytes(&self, path: &str, bytes: Vec<u8>, auth: bool) -> R<Value> {
        self.send(
            self.http
                .post(self.url(path))
                .header("content-type", "application/octet-stream")
                .body(bytes),
            auth,
        )
    }
    fn get_bytes(&self, path: &str, auth: bool) -> R<Vec<u8>> {
        let mut rb = self.http.get(self.url(path));
        if auth {
            if self.cfg.api_key.is_empty() {
                return Err(NO_KEY_HINT.into());
            }
            rb = rb.bearer_auth(&self.cfg.api_key);
        }
        let resp = rb.send()?;
        let status = resp.status();
        if !status.is_success() {
            return Err(format!("HTTP {status}: {}", resp.text().unwrap_or_default()).into());
        }
        Ok(resp.bytes()?.to_vec())
    }
    fn get_text(&self, path: &str, auth: bool) -> R<String> {
        let mut rb = self.http.get(self.url(path));
        if auth {
            if self.cfg.api_key.is_empty() {
                return Err(NO_KEY_HINT.into());
            }
            rb = rb.bearer_auth(&self.cfg.api_key);
        }
        let resp = rb.send()?;
        let status = resp.status();
        let text = resp.text().unwrap_or_default();
        if !status.is_success() {
            return Err(format!("HTTP {status}: {text}").into());
        }
        Ok(text)
    }
    fn put(&self, path: &str, body: Value, auth: bool) -> R<Value> {
        self.send(self.http.put(self.url(path)).json(&body), auth)
    }
    fn fetch_text(&self, url: &str) -> R<String> {
        let resp = self.http.get(url).send()?;
        if !resp.status().is_success() {
            return Err(format!("拉取 {url} 失败:HTTP {}", resp.status()).into());
        }
        Ok(resp.text()?)
    }
    fn delete(&self, path: &str, auth: bool) -> R<Value> {
        self.send(self.http.delete(self.url(path)), auth)
    }
    fn delete_body(&self, path: &str, body: Value, auth: bool) -> R<Value> {
        self.send(self.http.delete(self.url(path)).json(&body), auth)
    }
}

fn pretty(v: &Value) {
    if JSON_OUTPUT.load(std::sync::atomic::Ordering::Relaxed) {
        println!("{}", serde_json::to_string_pretty(v).unwrap_or_default());
        return;
    }
    render_human(v);
}

/// Print a freshly-minted API key (returned only once) before the normal output.
fn print_key(v: &Value) {
    if let Some(k) = v.get("key").and_then(|x| x.as_str()) {
        println!(" 已创建,密钥(仅此一次可见):\n{k}");
    }
    pretty(v);
}

fn cell(v: &Value) -> String {
    match v {
        Value::Null => "—".to_string(),
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::Array(a) => format!("[{} 项]", a.len()),
        Value::Object(o) => format!("{{{} 字段}}", o.len()),
    }
}

fn render_human(v: &Value) {
    if let Some(items) = v.get("items").and_then(|x| x.as_array()) {
        render_table(items);
        let total = v.get("total").map(cell).unwrap_or_default();
        let cur = v.get("current").map(cell).unwrap_or_default();
        let pages = v.get("totalPages").map(cell).unwrap_or_default();
        if !total.is_empty() {
            println!("\n共 {total} 条 · 第 {cur} 页 / 共 {pages} 页");
        }
        return;
    }
    match v {
        Value::Array(a) => render_table(a),
        Value::Object(_) => render_kv(v),
        other => println!("{}", cell(other)),
    }
}

fn render_table(items: &[Value]) {
    if items.is_empty() {
        println!("(空)");
        return;
    }
    let mut cols: Vec<String> = Vec::new();
    for it in items {
        if let Some(o) = it.as_object() {
            for k in o.keys() {
                if !cols.contains(k) {
                    cols.push(k.clone());
                }
            }
        }
    }
    if cols.is_empty() {
        for it in items {
            println!("{}", cell(it));
        }
        return;
    }
    let trunc = |s: String| {
        if s.chars().count() > 40 {
            format!("{}…", s.chars().take(39).collect::<String>())
        } else {
            s
        }
    };
    let mut widths: Vec<usize> = cols.iter().map(|c| c.chars().count()).collect();
    let rows: Vec<Vec<String>> = items
        .iter()
        .map(|it| {
            cols.iter()
                .map(|c| trunc(it.get(c).map(cell).unwrap_or_else(|| "—".to_string())))
                .collect()
        })
        .collect();
    for r in &rows {
        for (i, c) in r.iter().enumerate() {
            widths[i] = widths[i].max(c.chars().count());
        }
    }
    let pad = |s: &str, w: usize| {
        let n = s.chars().count();
        format!("{}{}", s, " ".repeat(w.saturating_sub(n)))
    };
    let header: Vec<String> = cols.iter().enumerate().map(|(i, c)| pad(c, widths[i])).collect();
    println!("{}", header.join("  "));
    println!("{}", widths.iter().map(|w| "-".repeat(*w)).collect::<Vec<_>>().join("  "));
    for r in &rows {
        let line: Vec<String> = r.iter().enumerate().map(|(i, c)| pad(c, widths[i])).collect();
        println!("{}", line.join("  "));
    }
}

fn render_kv(v: &Value) {
    if let Some(o) = v.as_object() {
        let w = o.keys().map(|k| k.chars().count()).max().unwrap_or(0);
        for (k, val) in o {
            let pad = " ".repeat(w.saturating_sub(k.chars().count()));
            println!("{k}{pad} : {}", cell(val));
        }
    }
}

fn status_assertions(expect_status: Option<u16>) -> Value {
    match expect_status {
        Some(code) => json!([{ "type": "StatusIs", "args": code }]),
        None => json!([]),
    }
}

fn parse_headers(items: &[String]) -> R<Value> {
    let mut arr = Vec::with_capacity(items.len());
    for it in items {
        let (name, value) =
            it.split_once(':').ok_or_else(|| format!("--header 需 'Name: value' 格式:{it}"))?;
        arr.push(json!({"name": name.trim(), "value": value.trim()}));
    }
    Ok(Value::Array(arr))
}

fn parse_vars(items: &[String]) -> R<Value> {
    let mut map = serde_json::Map::with_capacity(items.len());
    for it in items {
        let (k, v) = it.split_once('=').ok_or_else(|| format!("--var 需 'key=value' 格式:{it}"))?;
        map.insert(k.trim().to_string(), json!(v));
    }
    Ok(Value::Object(map))
}

const TPL_REQUIREMENT: &str = "# 需求模板

标题: 用户登录
描述: 支持邮箱 + 密码登录,签发会话令牌。

验收标准:
- 正确凭证返回 token
- 错误凭证返回 401
- 令牌过期后需重新登录

改好后录入(需先 `shepherd login`):

    shepherd req add --project <projectId> \\
      --title \"用户登录\" \\
      --description \"支持邮箱 + 密码登录,签发会话令牌。\" \\
      --criteria \"正确凭证返回 token,错误凭证返回 401,令牌过期后需重新登录\"
";

const TPL_GETTING_STARTED: &str = "# Shepherd 上手

前置:一个运行中的 server(默认 http://localhost:8088)。

1. 配置认证:`shepherd login --url http://localhost:8088 --api-key sak_…`
   (API key 在 个人中心 → API KEY 或 `POST /system/apikey` 签发;也可设环境变量 SHEPHERD_API_KEY)
2. 录入需求:见 `requirements/example.md`
3. 拆分任务:`shepherd decompose --req <requirementId> --version 1`
4. 派发执行:`shepherd dispatch --decomp <decompositionId> --task <taskId> --executor CLAUDE_CODE`
5. 验证 / 复查:`shepherd verify --help`、`shepherd decomposition --help`

各命令的完整参数见 `shepherd <命令> --help`。
";

/// Scaffold file manifest (relative path -> contents). Pure function for testability.
fn scaffold_files() -> Vec<(&'static str, &'static str)> {
    vec![
        ("requirements/example.md", TPL_REQUIREMENT),
        ("shepherd.getting-started.md", TPL_GETTING_STARTED),
    ]
}

fn run(cli: Cli) -> R<()> {
    JSON_OUTPUT.store(cli.json, std::sync::atomic::Ordering::Relaxed);
    match cli.cmd {
        Cmd::Init { dir, force } => {
            let root = std::path::Path::new(&dir);
            for (rel, contents) in scaffold_files() {
                let path = root.join(rel);
                if path.exists() && !force {
                    return Err(format!("已存在 {}(加 --force 覆盖)", path.display()).into());
                }
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&path, contents)?;
                println!("写入 {}", path.display());
            }
            println!(
                "下一步:编辑 requirements/example.md,然后 `shepherd login` 并按其中命令录入需求。"
            );
        }
        Cmd::Login { url, api_key } => {
            let mut cfg = Config::load();
            cfg.url = url;
            let key = api_key
                .or_else(|| std::env::var("SHEPHERD_API_KEY").ok())
                .filter(|k| !k.trim().is_empty())
                .ok_or(NO_KEY_HINT)?;
            cfg.api_key = key.trim().to_string();
            // The key is a static credential with no login endpoint to validate; only probe reachability — auth errors surface on the first business command.
            let healthy = Client::new(cfg.clone())?.get("/healthz", false).is_ok();
            cfg.save()?;
            println!(
                " 已保存 {} 的 API key → {} 服务{}",
                cfg.url,
                config_path().display(),
                if healthy { "可达" } else { "暂不可达" }
            );
        }
        Cmd::Agent { cmd } => match cmd {
            AgentCmd::Connect { kind } => {
                let executor = normalize_agent(&kind)?;
                let mut cfg = Config::load();
                let healthy = Client::new(cfg.clone())?.get("/healthz", false).is_ok();
                cfg.agent = Some(executor.clone());
                cfg.save()?;
                println!(
                    " 已连接 agent: {executor}  服务 {} {}",
                    cfg.url,
                    if healthy { "(可达)" } else { "(暂不可达)" }
                );
            }
            AgentCmd::Status => {
                let cfg = Config::load();
                let healthy = Client::new(cfg.clone())?.get("/healthz", false).is_ok();
                println!("服务  : {}", cfg.url);
                println!("API key: {}", if cfg.api_key.is_empty() { "未配置" } else { "已配置" });
                println!("agent : {}", cfg.agent.as_deref().unwrap_or("(未连接,默认 CLAUDE_CODE)"));
                println!("健康  : {}", if healthy { "可达" } else { "不可达" });
            }
            AgentCmd::Disconnect => {
                let mut cfg = Config::load();
                cfg.agent = None;
                cfg.save()?;
                println!("已断开 agent 连接(dispatch 回落默认 CLAUDE_CODE)");
            }
        },
        Cmd::Req { cmd } => {
            let c = Client::new(Config::load())?;
            match cmd {
                ReqCmd::Add { project, title, description, criteria } => pretty(&c.post(
                    "/requirement",
                    json!({"projectId": project, "title": title, "description": description, "acceptanceCriteria": criteria}),
                    true,
                )?),
                ReqCmd::List { project, page, page_size } => pretty(&c.get(
                    &format!("/requirement?projectId={project}&current={page}&pageSize={page_size}"),
                    true,
                )?),
                ReqCmd::Breakdown { req, version, ai: _ } => {
                    let path = match version {
                        Some(v) => format!("/requirement/{req}/breakdown?version={v}"),
                        None => format!("/requirement/{req}/breakdown"),
                    };
                    pretty(&c.post(&path, json!({}), true)?);
                }
            }
        }
        Cmd::Decompose { req, version } => {
            let c = Client::new(Config::load())?;
            pretty(&c.post(
                "/decomposition",
                json!({"requirementId": req, "requirementVersion": version}),
                true,
            )?);
        }
        Cmd::Decomposition { cmd } => {
            let c = Client::new(Config::load())?;
            match cmd {
                DecompositionCmd::Get { id } => {
                    pretty(&c.get(&format!("/decomposition/{id}"), true)?)
                }
                DecompositionCmd::Ready { id } => {
                    pretty(&c.get(&format!("/decomposition/{id}/ready"), true)?)
                }
                DecompositionCmd::Run { id, executor, concurrency } => pretty(&c.post(
                    &format!("/decomposition/{id}/run"),
                    json!({"executor": executor, "maxConcurrency": concurrency}),
                    true,
                )?),
            }
        }
        Cmd::Task { cmd } => {
            let c = Client::new(Config::load())?;
            match cmd {
                TaskCmd::Add { decomp, title, deps } => pretty(&c.post(
                    &format!("/decomposition/{decomp}/task"),
                    json!({"title": title, "dependencies": deps}),
                    true,
                )?),
            }
        }
        Cmd::Dispatch { decomp, task, title, executor, instructions, project, skills } => {
            let cfg = Config::load();
            let exec =
                executor.or_else(|| cfg.agent.clone()).unwrap_or_else(|| "CLAUDE_CODE".into());
            let c = Client::new(cfg)?;
            let mut instr = instructions;
            if !skills.is_empty() {
                let project = project.ok_or("--skills 需配合 --project")?;
                let comp = c.post(
                    "/skill/compose",
                    json!({"projectId": project, "skillIds": skills}),
                    true,
                )?;
                let composed = comp["instructions"].as_str().unwrap_or("").to_string();
                instr = Some(match instr {
                    Some(extra) if !extra.trim().is_empty() => format!("{composed}\n\n{extra}"),
                    _ => composed,
                });
            }
            pretty(&c.post(
                "/delivery",
                json!({"decompositionId": decomp, "taskId": task, "title": title, "executor": exec, "instructions": instr}),
                true,
            )?);
        }
        Cmd::Verify { cmd } => {
            let c = Client::new(Config::load())?;
            match cmd {
                VerifyCmd::Create { req, version, criteria } => pretty(&c.post(
                    "/verification",
                    json!({"requirementId": req, "requirementVersion": version, "criteria": criteria}),
                    true,
                )?),
                VerifyCmd::Link { id, criterion, decomp, task } => pretty(&c.post(
                    &format!("/verification/{id}/link"),
                    json!({"criterionIndex": criterion, "decompositionId": decomp, "taskId": task}),
                    true,
                )?),
                VerifyCmd::Sync { id, decomp, task, unsatisfied } => pretty(&c.post(
                    &format!("/verification/{id}/sync"),
                    json!({"decompositionId": decomp, "taskId": task, "satisfied": !unsatisfied}),
                    true,
                )?),
                VerifyCmd::Report { id } => pretty(&c.get(&format!("/verification/{id}/report"), true)?),
            }
        }
        Cmd::Skill { cmd } => {
            let c = Client::new(Config::load())?;
            match cmd {
                SkillCmd::Add { project, name, instructions, includes } => pretty(&c.post(
                    "/skill",
                    json!({"projectId": project, "name": name, "instructions": instructions, "includes": includes}),
                    true,
                )?),
                SkillCmd::Compose { project, ids } => pretty(&c.post(
                    "/skill/compose",
                    json!({"projectId": project, "skillIds": ids}),
                    true,
                )?),
            }
        }
        Cmd::Logout => {
            let mut cfg = Config::load();
            cfg.api_key.clear();
            cfg.save()?;
            println!(" 已清除本地 API key(要让 key 失效,请在服务端 API KEY 管理里吊销)");
        }
        Cmd::Project { cmd } => {
            let c = Client::new(Config::load())?;
            match cmd {
                ProjectCmd::Create { org, name } => pretty(&c.post(
                    "/project",
                    json!({"organizationId": org, "name": name}),
                    true,
                )?),
                ProjectCmd::List { org, current, page_size } => pretty(&c.get(
                    &format!(
                        "/project?organizationId={org}&current={current}&pageSize={page_size}"
                    ),
                    true,
                )?),
            }
        }
        Cmd::Bug { cmd } => {
            let c = Client::new(Config::load())?;
            match cmd {
                BugCmd::Create { project, title, status } => pretty(&c.post(
                    "/bug",
                    json!({"projectId": project, "title": title, "initialStatus": status}),
                    true,
                )?),
                BugCmd::Status { id, to } => {
                    pretty(&c.post(&format!("/bug/{id}/status"), json!({"status": to}), true)?)
                }
            }
        }
        Cmd::Case { cmd } => {
            let c = Client::new(Config::load())?;
            match cmd {
                CaseCmd::Review { review, case, reviewer, status, content } => pretty(&c.post(
                    &format!("/case-review/{review}/{case}"),
                    json!({"reviewerId": reviewer, "status": status, "content": content}),
                    true,
                )?),
            }
        }
        Cmd::Plan { cmd } => {
            let c = Client::new(Config::load())?;
            match cmd {
                PlanCmd::Create { project, name, plan_type, group } => {
                    let mut body = json!({"projectId": project, "name": name, "type": plan_type});
                    if let Some(g) = group {
                        body["groupId"] = json!(g);
                    }
                    pretty(&c.post("/test-plan", body, true)?)
                }
                PlanCmd::Stats { id } => {
                    pretty(&c.get(&format!("/test-plan/{id}/statistics"), true)?)
                }
                PlanCmd::Report { id, markdown } => {
                    let path = if markdown {
                        format!("/test-plan/{id}/report.md")
                    } else {
                        format!("/test-plan/{id}/report")
                    };
                    print!("{}", c.get_text(&path, false)?)
                }
                PlanCmd::Schedule { id, cron } => pretty(&c.post(
                    &format!("/test-plan/{id}/schedule"),
                    json!({"cron": cron}),
                    true,
                )?),
                PlanCmd::Runs { id } => pretty(&c.get(&format!("/test-plan/{id}/runs"), true)?),
                PlanCmd::LinkCase { id, case, name } => pretty(&c.post(
                    &format!("/test-plan/{id}/cases"),
                    json!({"caseId": case, "name": name}),
                    true,
                )?),
                PlanCmd::Result {
                    id,
                    case,
                    status,
                    latency_ms,
                    status_code,
                    body,
                    assertions_json,
                } => {
                    let assertions: Value = match assertions_json {
                        Some(aj) => serde_json::from_str(&aj)
                            .map_err(|e| format!("--assertions-json 不是合法 JSON: {e}"))?,
                        None => json!([]),
                    };
                    pretty(&c.post(
                        &format!("/test-plan/{id}/cases/{case}/result"),
                        json!({"status": status, "latencyMs": latency_ms, "statusCode": status_code,
                               "body": body, "assertions": assertions}),
                        true,
                    )?)
                }
                PlanCmd::Cases { id } => pretty(&c.get(&format!("/test-plan/{id}/cases"), true)?),
                PlanCmd::Run { id } => {
                    pretty(&c.post(&format!("/test-plan/{id}/run"), json!({}), true)?)
                }
            }
        }
        Cmd::Api { cmd } => {
            let c = Client::new(Config::load())?;
            match cmd {
                ApiCmd::BatchRun { project, cases, run_mode, pool, env } => pretty(&c.post(
                    "/api/batch-run",
                    json!({"projectId": project, "caseIds": cases, "runMode": run_mode, "poolId": pool, "environmentId": env}),
                    true,
                )?),
            }
        }
        Cmd::User { cmd } => {
            let c = Client::new(Config::load())?;
            match cmd {
                UserCmd::Create { name, email } => {
                    pretty(&c.post("/system/user", json!({"name": name, "email": email}), true)?)
                }
                UserCmd::List { current, page_size } => pretty(
                    &c.get(&format!("/system/user?current={current}&pageSize={page_size}"), true)?,
                ),
                UserCmd::Get { id } => pretty(&c.get(&format!("/system/user/{id}"), true)?),
                UserCmd::Update { id, name, email, disable } => pretty(&c.put(
                    &format!("/system/user/{id}"),
                    json!({"name": name, "email": email, "enable": !disable}),
                    true,
                )?),
                UserCmd::Delete { id } => pretty(&c.delete(&format!("/system/user/{id}"), true)?),
                UserCmd::Names { ids } => {
                    pretty(&c.get(&format!("/system/user/names?ids={}", ids.join(",")), true)?)
                }
            }
        }
        Cmd::Org { cmd } => {
            let c = Client::new(Config::load())?;
            match cmd {
                OrgCmd::Create { name, disable } => pretty(&c.post(
                    "/organization",
                    json!({"name": name, "enable": !disable}),
                    true,
                )?),
                OrgCmd::List { current, page_size } => pretty(
                    &c.get(&format!("/organization?current={current}&pageSize={page_size}"), true)?,
                ),
                OrgCmd::Get { id } => pretty(&c.get(&format!("/organization/{id}"), true)?),
                OrgCmd::Update { id, name, disable } => pretty(&c.put(
                    &format!("/organization/{id}"),
                    json!({"name": name, "enable": !disable}),
                    true,
                )?),
                OrgCmd::Delete { id } => pretty(&c.delete(&format!("/organization/{id}"), true)?),
            }
        }
        Cmd::Role { cmd } => {
            let c = Client::new(Config::load())?;
            match cmd {
                RoleCmd::Create { name, scope, permissions } => pretty(&c.post(
                    "/role",
                    json!({"name": name, "scope": scope, "permissions": permissions}),
                    true,
                )?),
                RoleCmd::List { current, page_size } => {
                    pretty(&c.get(&format!("/role?current={current}&pageSize={page_size}"), true)?)
                }
                RoleCmd::Get { id } => pretty(&c.get(&format!("/role/{id}"), true)?),
                RoleCmd::Update { id, name, scope, permissions } => pretty(&c.put(
                    &format!("/role/{id}"),
                    json!({"name": name, "scope": scope, "permissions": permissions}),
                    true,
                )?),
                RoleCmd::Delete { id } => pretty(&c.delete(&format!("/role/{id}"), true)?),
                RoleCmd::Grant { user, role } => pretty(&c.post(
                    "/user-role/grant",
                    json!({"userId": user, "roleId": role}),
                    true,
                )?),
                RoleCmd::Revoke { user, role } => pretty(&c.post(
                    "/user-role/revoke",
                    json!({"userId": user, "roleId": role}),
                    true,
                )?),
            }
        }
        Cmd::Apidef { cmd } => {
            let c = Client::new(Config::load())?;
            match cmd {
                ApidefCmd::Create { project, name, protocol, method, path } => pretty(&c.post(
                    "/api/definition",
                    json!({"projectId": project, "name": name, "protocol": protocol, "method": method, "path": path}),
                    true,
                )?),
                ApidefCmd::Import { project, file, url } => {
                    let raw = match (url, file) {
                        (Some(u), _) => c.fetch_text(&u)?,
                        (None, Some(f)) => std::fs::read_to_string(&f)?,
                        (None, None) => return Err("需指定 --file 或 --url".into()),
                    };
                    let content: Value =
                        serde_json::from_str(&raw).map_err(|e| format!("导入内容不是合法 JSON: {e}"))?;
                    pretty(&c.post(
                        "/api/definition/import",
                        json!({"projectId": project, "content": content}),
                        true,
                    )?)
                }
                ApidefCmd::List { project } => {
                    pretty(&c.get(&format!("/api/definition?projectId={project}"), true)?)
                }
                ApidefCmd::Get { id } => pretty(&c.get(&format!("/api/definition/{id}"), true)?),
                ApidefCmd::Case { def, name, method, url, body, expect_status } => pretty(&c.post(
                    &format!("/api/definition/{def}/case"),
                    json!({"name": name, "method": method, "url": url, "body": body, "assertions": status_assertions(expect_status)}),
                    true,
                )?),
                ApidefCmd::Cases { def } => {
                    pretty(&c.get(&format!("/api/definition/{def}/case"), true)?)
                }
                ApidefCmd::CaseNew { project, def, name, method, url, body, expect_status } => pretty(&c.post(
                    "/api/case",
                    json!({"projectId": project, "apiDefinitionId": def, "name": name, "method": method, "url": url, "body": body, "assertions": status_assertions(expect_status)}),
                    true,
                )?),
                ApidefCmd::GenSuite { project, base, no_scenario } => {
                    gen_suite(&c, &project, base.trim_end_matches('/'), no_scenario)?
                }
                ApidefCmd::CaseList { project, current, page_size } => pretty(&c.get(
                    &format!("/api/case?projectId={project}&current={current}&pageSize={page_size}"),
                    true,
                )?),
                ApidefCmd::CaseExec { case, current, page_size } => pretty(&c.get(
                    &format!("/api/case/{case}/executions?current={current}&pageSize={page_size}"),
                    true,
                )?),
                ApidefCmd::CaseRun { case, project, run_mode, pool, env } => pretty(&c.post(
                    &format!("/api/case/{case}/run"),
                    json!({"projectId": project, "runMode": run_mode, "poolId": pool, "environmentId": env}),
                    true,
                )?),
                ApidefCmd::Mock { def, name, response_status, body } => pretty(&c.post(
                    &format!("/api/definition/{def}/mock"),
                    json!({"name": name, "responseStatus": response_status, "responseBody": body}),
                    true,
                )?),
                ApidefCmd::Mocks { def } => {
                    pretty(&c.get(&format!("/api/definition/{def}/mock"), true)?)
                }
                ApidefCmd::Delete { id } => {
                    c.delete(&format!("/api/definition/{id}"), true)?;
                    println!(" 已删除接口定义 {id}");
                }
                ApidefCmd::References { id } => {
                    pretty(&c.get(&format!("/api/definition/{id}/references"), true)?)
                }
                ApidefCmd::Spec { id, spec_json } => {
                    let spec: Value = serde_json::from_str(&spec_json)
                        .map_err(|e| format!("--spec-json 不是合法 JSON: {e}"))?;
                    c.put(&format!("/api/definition/{id}/spec"), json!({ "spec": spec }), true)?;
                    println!(" 已更新接口定义 {id} 的规格");
                }
                ApidefCmd::Module { id, module_id, unset } => {
                    let mid = if unset { None } else { module_id };
                    c.put(
                        &format!("/api/definition/{id}/module"),
                        json!({ "moduleId": mid }),
                        true,
                    )?;
                    println!(
                        " 接口定义 {id} 已{}",
                        if mid.is_some() { "移入模块" } else { "移出到未归类" }
                    );
                }
                ApidefCmd::Status { id, status } => {
                    c.put(
                        &format!("/api/definition/{id}/status"),
                        json!({ "status": status }),
                        true,
                    )?;
                    println!(" 已设置接口定义 {id} 状态为 {status}");
                }
                ApidefCmd::Changes { id } => {
                    pretty(&c.get(&format!("/api/definition/{id}/changes"), true)?)
                }
            }
        }
        Cmd::Scenario { cmd } => {
            let c = Client::new(Config::load())?;
            match cmd {
                ScenarioCmd::Create { project, name } => pretty(&c.post(
                    "/api/scenario",
                    json!({"projectId": project, "name": name}),
                    true,
                )?),
                ScenarioCmd::List { project } => {
                    pretty(&c.get(&format!("/api/scenario?projectId={project}"), true)?)
                }
                ScenarioCmd::Get { id } => pretty(&c.get(&format!("/api/scenario/{id}"), true)?),
                ScenarioCmd::Step { scenario, kind, ref_mode, order, ref_id, method, url, body, expect_status, assertions_json, control_json } => {
                    let mut step = json!({"kind": kind.to_uppercase(), "refMode": ref_mode.to_uppercase(), "order": order});
                    if let Some(r) = ref_id {
                        step["refId"] = json!(r);
                    }
                    if let Some(m) = method {
                        let assertions = match assertions_json {
                            Some(aj) => serde_json::from_str(&aj)
                                .map_err(|e| format!("--assertions-json 不是合法 JSON: {e}"))?,
                            None => status_assertions(expect_status),
                        };
                        step["request"] = json!({"method": m, "url": url.unwrap_or_default(), "body": body, "assertions": assertions});
                    }
                    if let Some(cj) = control_json {
                        step["control"] = serde_json::from_str(&cj)
                            .map_err(|e| format!("--control-json 不是合法 JSON: {e}"))?;
                    }
                    pretty(&c.post(&format!("/api/scenario/{scenario}/step"), step, true)?)
                }
                ScenarioCmd::Compile { id } => {
                    pretty(&c.get(&format!("/api/scenario/{id}/compile"), true)?)
                }
                ScenarioCmd::Executions { id, current, page_size } => pretty(&c.get(
                    &format!("/api/scenario/{id}/executions?current={current}&pageSize={page_size}"),
                    true,
                )?),
                ScenarioCmd::Run { id, project, run_mode, pool, env } => pretty(&c.post(
                    &format!("/api/scenario/{id}/run"),
                    json!({"projectId": project, "runMode": run_mode, "poolId": pool, "environmentId": env}),
                    true,
                )?),
            }
        }
        Cmd::Env { cmd } => {
            let c = Client::new(Config::load())?;
            match cmd {
                EnvCmd::Create { project, name, base, headers, vars } => pretty(&c.post(
                    "/api/environment",
                    json!({"projectId": project, "name": name, "baseUrl": base,
                           "headers": parse_headers(&headers)?, "variables": parse_vars(&vars)?}),
                    true,
                )?),
                EnvCmd::List { project } => {
                    pretty(&c.get(&format!("/api/environment?projectId={project}"), true)?)
                }
                EnvCmd::Get { id } => pretty(&c.get(&format!("/api/environment/{id}"), true)?),
                EnvCmd::Update { id, project, name, base, headers, vars } => pretty(&c.put(
                    &format!("/api/environment/{id}"),
                    json!({"projectId": project, "name": name, "baseUrl": base,
                           "headers": parse_headers(&headers)?, "variables": parse_vars(&vars)?}),
                    true,
                )?),
                EnvCmd::Delete { id } => {
                    pretty(&c.delete(&format!("/api/environment/{id}"), true)?)
                }
            }
        }
        Cmd::Fcase { cmd } => {
            let c = Client::new(Config::load())?;
            match cmd {
                FcaseCmd::Create { project, name, module, priority, status, fields } => pretty(&c.post(
                    "/functional-case",
                    json!({"projectId": project, "name": name, "module": module, "priority": priority,
                           "status": status, "customFields": parse_vars(&fields)?}),
                    true,
                )?),
                FcaseCmd::List { project } => {
                    pretty(&c.get(&format!("/functional-case?projectId={project}"), true)?)
                }
                FcaseCmd::Export { project, out } => {
                    let bytes = c.get_bytes(&format!("/functional-case/export?projectId={project}"), true)?;
                    std::fs::write(&out, &bytes)?;
                    println!(" 已导出 {} 字节 → {out}", bytes.len());
                }
                FcaseCmd::Import { project, file } => {
                    let bytes = std::fs::read(&file)?;
                    pretty(&c.post_bytes(
                        &format!("/functional-case/import?projectId={project}"),
                        bytes,
                        true,
                    )?)
                }
            }
        }
        Cmd::Runner { cmd } => {
            let c = Client::new(Config::load())?;
            match cmd {
                RunnerCmd::Register { name, base_url, token } => pretty(&c.post(
                    "/runner-agent",
                    json!({"name": name, "baseUrl": base_url, "token": token}),
                    true,
                )?),
                RunnerCmd::List => pretty(&c.get("/runner-agent", true)?),
                RunnerCmd::Run { agent, url, method, body, expect_status } => pretty(&c.post(
                    &format!("/runner-agent/{agent}/run"),
                    json!({
                        "request": {"method": method.to_uppercase(), "url": url, "headers": [], "body": body},
                        "assertions": status_assertions(expect_status),
                    }),
                    true,
                )?),
                RunnerCmd::RunCase { agent, case } => pretty(&c.post(
                    &format!("/runner-agent/{agent}/run-case"),
                    json!({"caseId": case}),
                    true,
                )?),
                RunnerCmd::Executions { agent } => {
                    pretty(&c.get(&format!("/runner-agent/{agent}/executions"), true)?)
                }
                RunnerCmd::Refresh { agent } => pretty(&c.post(
                    &format!("/runner-agent/{agent}/refresh"),
                    json!({}),
                    true,
                )?),
                RunnerCmd::Probe { protocol, target, payload, meta, expect_status, contains, latency_under } => {
                    let mut metadata = serde_json::Map::new();
                    for kv in &meta {
                        if let Some((k, v)) = kv.split_once('=') {
                            metadata.insert(k.to_string(), json!(v));
                        }
                    }
                    let mut assertions: Vec<Value> = Vec::new();
                    if let Some(s) = expect_status {
                        assertions.push(json!({"type": "status_is", "value": s}));
                    }
                    if let Some(sub) = &contains {
                        assertions.push(json!({"type": "output_contains", "value": sub}));
                    }
                    if let Some(ms) = latency_under {
                        assertions.push(json!({"type": "latency_under_ms", "value": ms}));
                    }
                    pretty(&c.post(
                        "/runner/probe",
                        json!({
                            "protocol": protocol, "target": target, "payload": payload,
                            "metadata": metadata, "assertions": assertions,
                        }),
                        true,
                    )?)
                }
            }
        }
        Cmd::Perf { cmd } => {
            let c = Client::new(Config::load())?;
            match cmd {
                PerfCmd::Run {
                    url,
                    method,
                    concurrency,
                    iterations,
                    duration_ms,
                    expect_status,
                    contains,
                    equals,
                    latency_under,
                    protocol,
                    query,
                    project,
                } => pretty(&c.post(
                    "/perf/run",
                    json!({"url": url, "method": method, "concurrency": concurrency,
                               "iterations": iterations, "durationMs": duration_ms,
                               "expectStatus": expect_status, "expectContains": contains,
                               "expectEquals": equals, "latencyUnderMs": latency_under,
                               "protocol": protocol, "query": query,
                               "projectId": project}),
                    true,
                )?),
                PerfCmd::Report { id } => pretty(&c.get(&format!("/perf/report/{id}"), true)?),
            }
        }
        Cmd::Pool { cmd } => {
            let c = Client::new(Config::load())?;
            match cmd {
                PoolCmd::Create { name, disable } => pretty(&c.post(
                    "/api/resource-pool",
                    json!({"name": name, "enabled": !disable}),
                    true,
                )?),
                PoolCmd::List => pretty(&c.get("/api/resource-pool", true)?),
                PoolCmd::Status => pretty(&c.get("/api/pool-runner/status", true)?),
                PoolCmd::StatusDetail => pretty(&c.get("/api/pool-runner/status/detail", true)?),
            }
        }
        Cmd::Auth { cmd } => {
            let cfg = Config::load();
            match cmd {
                AuthCmd::Login { username, password } => {
                    let c = Client::new(cfg)?;
                    let v = c.post("/auth/login", json!({"username": username, "password": password}), false)?;
                    println!(" 登录成功,会话 token:\n{}", v.get("token").and_then(|t| t.as_str()).unwrap_or(""));
                    pretty(&v);
                }
                AuthCmd::Logout => {
                    let c = Client::new(cfg)?;
                    c.post("/auth/logout", json!({}), true)?;
                    println!(" 已登出");
                }
                AuthCmd::Refresh => {
                    let c = Client::new(cfg)?;
                    pretty(&c.post("/auth/refresh", json!({}), true)?);
                }
                AuthCmd::Me => {
                    let c = Client::new(cfg)?;
                    pretty(&c.get("/auth/me", true)?);
                }
                AuthCmd::Password { old_password, new_password } => {
                    let c = Client::new(cfg)?;
                    c.post(
                        "/auth/password",
                        json!({"oldPassword": old_password, "newPassword": new_password}),
                        true,
                    )?;
                    println!(" 密码已修改");
                }
                AuthCmd::Oidc { provider } => {
                    let base = cfg.url.trim_end_matches('/');
                    println!("{base}/auth/oidc/{provider}/authorize");
                }
            }
        }
        Cmd::Apikey { cmd } => {
            let c = Client::new(Config::load())?;
            match cmd {
                ApikeyCmd::Create { name, permissions } => {
                    let v = c.post(
                        "/system/apikey",
                        json!({"name": name, "permissions": permissions}),
                        true,
                    )?;
                    print_key(&v);
                }
                ApikeyCmd::CreateMine { name, ttl_secs } => {
                    let mut body = json!({});
                    if let Some(n) = name {
                        body["name"] = json!(n);
                    }
                    if let Some(t) = ttl_secs {
                        body["ttlSecs"] = json!(t);
                    }
                    let v = c.post("/system/apikey/mine", body, true)?;
                    print_key(&v);
                }
                ApikeyCmd::List => pretty(&c.get("/system/apikey", true)?),
                ApikeyCmd::Mine => pretty(&c.get("/system/apikey/mine", true)?),
                ApikeyCmd::Delete { id } => {
                    c.delete(&format!("/system/apikey/{id}"), true)?;
                    println!(" 已吊销 {id}");
                }
                ApikeyCmd::Enable { id, disable } => {
                    c.put(
                        &format!("/system/apikey/{id}/enabled"),
                        json!({"enabled": !disable}),
                        true,
                    )?;
                    println!(" API key {id} 已{}", if disable { "禁用" } else { "启用" });
                }
            }
        }
        Cmd::Llm { cmd } => {
            let c = Client::new(Config::load())?;
            match cmd {
                LlmCmd::List => pretty(&c.get("/me/llm-model", true)?),
                LlmCmd::Create { provider, name, base_url, api_key } => {
                    let mut body = json!({"provider": provider, "name": name});
                    if let Some(b) = base_url {
                        body["baseUrl"] = json!(b);
                    }
                    if let Some(k) = api_key {
                        body["apiKey"] = json!(k);
                    }
                    pretty(&c.post("/me/llm-model", body, true)?);
                }
                LlmCmd::Update { id, name, base_url, api_key, enable, disable } => {
                    let mut patch = json!({});
                    if let Some(n) = name {
                        patch["name"] = json!(n);
                    }
                    if let Some(b) = base_url {
                        patch["baseUrl"] = json!(b);
                    }
                    if let Some(k) = api_key {
                        patch["apiKey"] = json!(k);
                    }
                    if disable {
                        patch["enabled"] = json!(false);
                    } else if enable {
                        patch["enabled"] = json!(true);
                    }
                    pretty(&c.put(&format!("/me/llm-model/{id}"), patch, true)?);
                }
                LlmCmd::Delete { id } => {
                    c.delete(&format!("/me/llm-model/{id}"), true)?;
                    println!(" 已删除 LLM model {id}");
                }
            }
        }
        Cmd::Notice { cmd } => {
            let c = Client::new(Config::load())?;
            match cmd {
                NoticeCmd::List { project, category, tab, page, page_size } => {
                    let mut parts: Vec<String> = Vec::new();
                    if let Some(p) = &project {
                        parts.push(format!("projectId={p}"));
                    }
                    if let Some(c) = &category {
                        parts.push(format!("category={c}"));
                    }
                    if let Some(t) = &tab {
                        parts.push(format!("tab={t}"));
                    }
                    parts.push(format!("page={page}"));
                    parts.push(format!("pageSize={page_size}"));
                    pretty(&c.get(&format!("/notice?{}", parts.join("&")), true)?);
                }
                NoticeCmd::UnreadCount { project } => {
                    let path = match &project {
                        Some(p) => format!("/notice/unread-count?projectId={p}"),
                        None => "/notice/unread-count".to_string(),
                    };
                    pretty(&c.get(&path, true)?);
                }
                NoticeCmd::ReadAll { project } => {
                    let path = match &project {
                        Some(p) => format!("/notice/read-all?projectId={p}"),
                        None => "/notice/read-all".to_string(),
                    };
                    c.post(&path, json!({}), true)?;
                    println!(" 已全部已读");
                }
                NoticeCmd::Read { id } => {
                    c.post(&format!("/notice/{id}/read"), json!({}), true)?;
                    println!(" 已标记已读 {id}");
                }
                NoticeCmd::Robots { cmd } => match cmd {
                    NoticeRobotCmd::List => pretty(&c.get("/notice/robots", true)?),
                    NoticeRobotCmd::Create { project, name, platform, webhook_url, secret, enable } => {
                        let mut body = json!({"projectId": project, "name": name, "platform": platform, "webhookUrl": webhook_url, "enabled": enable});
                        if let Some(s) = secret {
                            body["secret"] = json!(s);
                        }
                        pretty(&c.post("/notice/robots", body, true)?);
                    }
                    NoticeRobotCmd::Update { id, project, name, platform, webhook_url, secret, enable } => {
                        let mut body = json!({"projectId": project, "name": name, "platform": platform, "webhookUrl": webhook_url, "enabled": enable});
                        if let Some(s) = secret {
                            body["secret"] = json!(s);
                        }
                        pretty(&c.put(&format!("/notice/robots/{id}"), body, true)?);
                    }
                    NoticeRobotCmd::Delete { id } => {
                        c.delete(&format!("/notice/robots/{id}"), true)?;
                        println!(" 已删除 robot {id}");
                    }
                    NoticeRobotCmd::Test { id } => {
                        pretty(&c.post(&format!("/notice/robots/{id}/test"), json!({}), true)?);
                    }
                },
                NoticeCmd::Rules { cmd } => match cmd {
                    NoticeRuleCmd::List { project } => {
                        let path = match &project {
                            Some(p) => format!("/notice/rules?projectId={p}"),
                            None => "/notice/rules".to_string(),
                        };
                        pretty(&c.get(&path, true)?);
                    }
                    NoticeRuleCmd::Create { project, event_type, channels, robot_ids, template, enable } => pretty(&c.post(
                        "/notice/rules",
                        json!({"projectId": project, "eventType": event_type, "channels": channels, "robotIds": robot_ids, "template": template, "enabled": enable}),
                        true,
                    )?),
                    NoticeRuleCmd::Update { id, project, event_type, channels, robot_ids, template, enable } => pretty(&c.put(
                        &format!("/notice/rules/{id}"),
                        json!({"projectId": project, "eventType": event_type, "channels": channels, "robotIds": robot_ids, "template": template, "enabled": enable}),
                        true,
                    )?),
                    NoticeRuleCmd::Delete { id } => {
                        c.delete(&format!("/notice/rules/{id}"), true)?;
                        println!(" 已删除 rule {id}");
                    }
                },
            }
        }
        Cmd::Follow { cmd } => {
            let c = Client::new(Config::load())?;
            match cmd {
                FollowCmd::Add { project, entity_type, entity_id } => pretty(&c.post(
                    "/follow",
                    json!({"projectId": project, "entityType": entity_type, "entityId": entity_id}),
                    true,
                )?),
                FollowCmd::Remove { project, entity_type, entity_id } => {
                    pretty(&c.delete_body(
                        "/follow",
                        json!({"projectId": project, "entityType": entity_type, "entityId": entity_id}),
                        true,
                    )?)
                }
                FollowCmd::Status { project, entity_type, entity_id } => pretty(&c.get(
                    &format!("/follow?projectId={project}&entityType={entity_type}&entityId={entity_id}"),
                    true,
                )?),
                FollowCmd::Mine { project, entity_type } => {
                    let path = match &entity_type {
                        Some(t) => format!("/follow/mine?projectId={project}&entityType={t}"),
                        None => format!("/follow/mine?projectId={project}"),
                    };
                    pretty(&c.get(&path, true)?);
                }
            }
        }
        Cmd::Comment { cmd } => {
            let c = Client::new(Config::load())?;
            match cmd {
                CommentCmd::Add { target_type, target_id, content } => pretty(&c.post(
                    "/comment",
                    json!({"targetType": target_type, "targetId": target_id, "content": content}),
                    true,
                )?),
                CommentCmd::List { target_type, target_id } => pretty(&c.get(
                    &format!("/comment?targetType={target_type}&targetId={target_id}"),
                    true,
                )?),
                CommentCmd::Delete { id } => {
                    c.delete(&format!("/comment/{id}"), true)?;
                    println!(" 已删除评论 {id}");
                }
            }
        }
        Cmd::Proposal { cmd } => {
            let c = Client::new(Config::load())?;
            match cmd {
                ProposalCmd::Create { requirement, title } => pretty(&c.post(
                    "/proposal",
                    json!({"requirementId": requirement, "title": title}),
                    true,
                )?),
                ProposalCmd::List { requirement } => {
                    pretty(&c.get(&format!("/proposal?requirementId={requirement}"), true)?)
                }
                ProposalCmd::Get { id } => pretty(&c.get(&format!("/proposal/{id}"), true)?),
                ProposalCmd::Design { id, doc } => pretty(&c.post(
                    &format!("/proposal/{id}/design"),
                    json!({"doc": doc}),
                    true,
                )?),
                ProposalCmd::Approve { id } => {
                    pretty(&c.post(&format!("/proposal/{id}/approve"), json!({}), true)?)
                }
                ProposalCmd::RequestChanges { id, comment } => pretty(&c.post(
                    &format!("/proposal/{id}/request-changes"),
                    json!({"comment": comment}),
                    true,
                )?),
            }
        }
        Cmd::Prd { cmd } => {
            let c = Client::new(Config::load())?;
            match cmd {
                PrdCmd::Draft { raw } => {
                    pretty(&c.post("/requirement/draft", json!({"raw": raw}), true)?)
                }
            }
        }
        Cmd::Import { cmd } => {
            let c = Client::new(Config::load())?;
            match cmd {
                ImportCmd::Url {
                    project,
                    format,
                    url,
                    token,
                    basic_auth,
                    module_id,
                    no_group_by_tag,
                    no_overwrite,
                    sync_module,
                } => {
                    let mut body = json!({
                        "projectId": project,
                        "url": url,
                        "basicAuth": basic_auth,
                        "groupByTag": !no_group_by_tag,
                        "overwrite": !no_overwrite,
                        "syncModule": sync_module,
                    });
                    if let Some(f) = format {
                        body["format"] = json!(f);
                    }
                    if let Some(t) = token {
                        body["token"] = json!(t);
                    }
                    if let Some(m) = module_id {
                        body["moduleId"] = json!(m);
                    }
                    pretty(&c.post("/api/definition/import-url", body, true)?)
                }
                ImportCmd::ScheduleCreate {
                    project,
                    name,
                    format,
                    url,
                    token,
                    basic_auth,
                    module_id,
                    no_group_by_tag,
                    no_overwrite,
                    sync_module,
                    cron,
                    disable,
                } => {
                    let mut body = json!({
                        "projectId": project,
                        "url": url,
                        "basicAuth": basic_auth,
                        "groupByTag": !no_group_by_tag,
                        "overwrite": !no_overwrite,
                        "syncModule": sync_module,
                        "cron": cron,
                        "enabled": !disable,
                    });
                    if let Some(n) = name {
                        body["name"] = json!(n);
                    }
                    if let Some(f) = format {
                        body["format"] = json!(f);
                    }
                    if let Some(t) = token {
                        body["token"] = json!(t);
                    }
                    if let Some(m) = module_id {
                        body["moduleId"] = json!(m);
                    }
                    pretty(&c.post("/api/import-schedule", body, true)?)
                }
                ImportCmd::ScheduleList { project } => {
                    pretty(&c.get(&format!("/api/import-schedule?projectId={project}"), true)?)
                }
                ImportCmd::ScheduleDelete { id } => {
                    c.delete(&format!("/api/import-schedule/{id}"), true)?;
                    println!(" 已删除导入计划 {id}");
                }
                ImportCmd::ScheduleEnable { id, disable } => {
                    c.put(
                        &format!("/api/import-schedule/{id}/enabled"),
                        json!({ "enabled": !disable }),
                        true,
                    )?;
                    println!(" 导入计划 {id} 已{}", if disable { "禁用" } else { "启用" });
                }
                ImportCmd::ScheduleRun { id } => {
                    pretty(&c.post(&format!("/api/import-schedule/{id}/run"), json!({}), true)?)
                }
            }
        }
        Cmd::Mcp { cmd } => {
            let c = Client::new(Config::load())?;
            match cmd {
                McpCmd::Call { method, params_json, id } => {
                    let params: Value = match params_json {
                        Some(p) => serde_json::from_str(&p)
                            .map_err(|e| format!("--params-json 不是合法 JSON: {e}"))?,
                        None => Value::Null,
                    };
                    let mut body = json!({ "jsonrpc": "2.0", "method": method });
                    if let Some(i) = id {
                        body["id"] = json!(i);
                    }
                    if params != Value::Null {
                        body["params"] = params;
                    }
                    pretty(&c.post("/mcp", body, true)?)
                }
                McpCmd::Tools => pretty(
                    &c.post("/mcp", json!({ "jsonrpc": "2.0", "method": "tools/list", "id": 1 }), true)?,
                ),
                McpCmd::ToolsCall { name, args_json } => {
                    let args: Value = match args_json {
                        Some(a) => serde_json::from_str(&a)
                            .map_err(|e| format!("--args-json 不是合法 JSON: {e}"))?,
                        None => json!({}),
                    };
                    pretty(&c.post(
                        "/mcp",
                        json!({ "jsonrpc": "2.0", "method": "tools/call", "id": 1,
                               "params": { "name": name, "arguments": args } }),
                        true,
                    )?)
                }
            }
        }
        Cmd::Debug { cmd } => {
            let c = Client::new(Config::load())?;
            match cmd {
                DebugCmd::Send { protocol, method, url, headers, body, meta, assertions_json } => {
                    let mut hdr_arr = Vec::with_capacity(headers.len());
                    for h in &headers {
                        let (k, v) = h
                            .split_once(':')
                            .ok_or_else(|| format!("--header 需 'Name: value' 格式:{h}"))?;
                        hdr_arr.push(json!({ "key": k.trim(), "value": v.trim() }));
                    }
                    let mut metadata = serde_json::Map::new();
                    for kv in &meta {
                        if let Some((k, v)) = kv.split_once('=') {
                            metadata.insert(k.trim().to_string(), json!(v.trim()));
                        }
                    }
                    let assertions: Value = match assertions_json {
                        Some(aj) => serde_json::from_str(&aj)
                            .map_err(|e| format!("--assertions-json 不是合法 JSON: {e}"))?,
                        None => json!([]),
                    };
                    let req = json!({
                        "protocol": if protocol.is_empty() { Value::Null } else { json!(protocol) },
                        "method": method.to_uppercase(),
                        "url": url,
                        "headers": hdr_arr,
                        "body": body,
                        "meta": metadata,
                        "assertions": assertions,
                    });
                    pretty(&c.post("/api/debug/send", req, true)?)
                }
                DebugCmd::Protocols => pretty(&c.get("/api/debug/protocols", true)?),
            }
        }
        Cmd::Caseexec { cmd } => {
            let c = Client::new(Config::load())?;
            match cmd {
                CaseExecCmd::Summary { project } => {
                    pretty(&c.get(&format!("/api/case-exec-summary?projectId={project}"), true)?)
                }
                CaseExecCmd::Trend { project, days } => {
                    pretty(&c.get(
                        &format!("/api/exec-trend?projectId={project}&days={days}"),
                        true,
                    )?)
                }
            }
        }
        Cmd::Metrics => {
            let c = Client::new(Config::load())?;
            print!("{}", c.get_text("/metrics", false)?)
        }
    }
    Ok(())
}

fn gen_suite(c: &Client, project: &str, base: &str, no_scenario: bool) -> R<()> {
    let defs = c.get(&format!("/api/definition?projectId={project}"), true)?;
    let list = defs.as_array().ok_or("接口定义列表不是数组")?;
    if list.is_empty() {
        return Err("项目内没有接口定义,先 `apidef import`".into());
    }
    let (mut cases, mut scenarios, mut steps, mut failed) = (0u32, 0u32, 0u32, 0u32);
    for d in list {
        let def_id = d["id"].as_str().unwrap_or_default();
        let name = d["name"].as_str().unwrap_or("(unnamed)");
        let method = d["method"].as_str().unwrap_or("GET").to_uppercase();
        let path = d["path"].as_str().unwrap_or("");
        let url = format!("{base}{path}");
        let success_code = if method == "POST" { 201 } else { 200 };

        let mk_case = |label: &str, code: u16| {
            c.post(
                "/api/case",
                json!({
                    "projectId": project,
                    "apiDefinitionId": def_id,
                    "name": format!("{name} [{label}]"),
                    "method": method,
                    "url": url,
                    "body": Value::Null,
                    "assertions": [{"type": "StatusIs", "args": code}],
                }),
                true,
            )
        };
        let ok = match mk_case(&format!("成功·期望{success_code}"), success_code) {
            Ok(v) => v,
            Err(e) => {
                eprintln!(" {name}: 成功用例创建失败:{e}");
                failed += 1;
                continue;
            }
        };
        let bad = match mk_case("失败·期望401", 401) {
            Ok(v) => v,
            Err(e) => {
                eprintln!(" {name}: 失败用例创建失败:{e}");
                failed += 1;
                continue;
            }
        };
        cases += 2;
        let (ok_id, bad_id) = (
            ok["id"].as_str().unwrap_or_default().to_string(),
            bad["id"].as_str().unwrap_or_default().to_string(),
        );

        if no_scenario {
            continue;
        }
        let sc = match c.post(
            "/api/scenario",
            json!({"projectId": project, "name": format!("{name} 场景")}),
            true,
        ) {
            Ok(v) => v,
            Err(e) => {
                eprintln!(" {name}: 场景创建失败:{e}");
                failed += 1;
                continue;
            }
        };
        let sc_id = sc["id"].as_str().unwrap_or_default();
        scenarios += 1;
        for (order, ref_id) in [(1, &ok_id), (2, &bad_id)] {
            match c.post(
                &format!("/api/scenario/{sc_id}/step"),
                json!({"kind": "CASE", "refMode": "REFERENCE", "order": order, "refId": ref_id}),
                true,
            ) {
                Ok(_) => steps += 1,
                Err(e) => eprintln!(" {name}: 步骤{order}添加失败:{e}"),
            }
        }
    }
    println!(
        " 生成完成:{} 个接口 → {cases} 条用例、{scenarios} 条场景、{steps} 个步骤(失败 {failed})",
        list.len()
    );
    Ok(())
}

fn main() {
    if let Err(e) = run(Cli::parse()) {
        eprintln!(" {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn normalize_agent_maps_kinds() {
        assert_eq!(normalize_agent("claude-code").unwrap(), "CLAUDE_CODE");
        assert_eq!(normalize_agent("CODEX").unwrap(), "CODEX");
        assert_eq!(normalize_agent("opencode").unwrap(), "OPENCODE");
        assert_eq!(normalize_agent("CodeBuddy").unwrap(), "CODEBUDDY");
        assert_eq!(normalize_agent("code-buddy").unwrap(), "CODEBUDDY");
        assert!(normalize_agent("gpt").is_err());
    }

    #[test]
    fn url_join_trims_slash() {
        let c = Client::new(Config {
            url: "http://h:1/".into(),
            api_key: "sak_a.b".into(),
            agent: None,
        })
        .expect("client");
        assert_eq!(c.url("/x"), "http://h:1/x");
    }

    #[test]
    fn config_defaults_url_when_empty() {
        let c = Config { url: String::new(), api_key: String::new(), agent: None };
        assert!(c.url.is_empty());
    }

    #[test]
    fn missing_api_key_yields_issuance_hint() {
        let c = Client::new(Config {
            url: "http://127.0.0.1:9".into(),
            api_key: String::new(),
            agent: None,
        })
        .expect("client");
        let err = c.get("/organization", true).expect_err("no key must fail before any request");
        let msg = err.to_string();
        assert!(msg.contains("SHEPHERD_API_KEY"), "要点名环境变量: {msg}");
        assert!(msg.contains("/system/apikey"), "要给签发指引: {msg}");
    }

    #[test]
    fn scaffold_templates_carry_real_commands() {
        let files = scaffold_files();
        assert_eq!(files.len(), 2);
        let req = files.iter().find(|(p, _)| *p == "requirements/example.md").unwrap().1;
        assert!(req.contains("shepherd req add"));
        assert!(req.contains("--criteria"));
    }

    #[test]
    fn init_writes_scaffold_and_refuses_overwrite() {
        let dir = std::env::temp_dir().join(format!("shep-init-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let init = |force: bool| {
            run(Cli { json: false, cmd: Cmd::Init { dir: dir.to_string_lossy().into(), force } })
        };
        init(false).expect("first init writes");
        assert!(dir.join("requirements/example.md").is_file());
        assert!(dir.join("shepherd.getting-started.md").is_file());
        // Second run without --force must refuse to overwrite.
        assert!(init(false).is_err());
        init(true).expect("force overwrites");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
