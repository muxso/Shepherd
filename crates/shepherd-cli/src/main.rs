//! Shepherd CLI client: drives the server over the REST API (requirements/tasks/deliveries, etc.).
//! Renderers human-readable output by default; --json emits raw JSON for scripting.
//!
//! The top-level [`Cli`]/[`Cmd`] only declare the command surface and dispatch; every group's
//! logic lives in its own `commands/<group>.rs` module (see `commands/mod.rs`).

mod client;
mod commands;

use clap::{Parser, Subcommand};

use crate::client::{JSON_OUTPUT, R};
use crate::commands::*;

#[derive(Parser)]
#[command(name = "shepherd", version, about = "Shepherd —— AI 研发监督平台 CLI")]
struct Cli {
    /// Output raw JSON (default renders human-readable tables / key-values). For scripts/pipes.
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    cmd: Cmd,
}

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
        cmd: req::ReqCmd,
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
        cmd: decomposition::DecompositionCmd,
    },
    /// Task management.
    Task {
        #[command(subcommand)]
        cmd: task::TaskCmd,
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
        cmd: verify::VerifyCmd,
    },
    /// AI skill orchestration.
    Skill {
        #[command(subcommand)]
        cmd: skill::SkillCmd,
    },
    /// Connect/inspect the AI executor (the default executor for dispatch).
    Agent {
        #[command(subcommand)]
        cmd: agent::AgentCmd,
    },
    /// Clear the locally saved API key (to invalidate the key, revoke it in the server's API KEY management).
    Logout,
    /// Project management.
    Project {
        #[command(subcommand)]
        cmd: project::ProjectCmd,
    },
    /// Bug management.
    Bug {
        #[command(subcommand)]
        cmd: bug::BugCmd,
    },
    /// Case review.
    Case {
        #[command(subcommand)]
        cmd: case::CaseCmd,
    },
    /// Test plans.
    Plan {
        #[command(subcommand)]
        cmd: plan::PlanCmd,
    },
    /// API testing (batch execution).
    Api {
        #[command(subcommand)]
        cmd: api::ApiCmd,
    },
    /// User management.
    User {
        #[command(subcommand)]
        cmd: user::UserCmd,
    },
    /// Organization management.
    Org {
        #[command(subcommand)]
        cmd: org::OrgCmd,
    },
    /// Roles and grants.
    Role {
        #[command(subcommand)]
        cmd: role::RoleCmd,
    },
    /// API definitions (catalog + cases + mocks).
    Apidef {
        #[command(subcommand)]
        cmd: apidef::ApidefCmd,
    },
    /// Scenarios (orchestration + steps + compile + run).
    Scenario {
        #[command(subcommand)]
        cmd: scenario::ScenarioCmd,
    },
    /// Environments (per-project base_url + default headers + variables; injected at run time).
    Env {
        #[command(subcommand)]
        cmd: env::EnvCmd,
    },
    /// Functional cases (CRUD + custom fields + Excel export).
    Fcase {
        #[command(subcommand)]
        cmd: fcase::FcaseCmd,
    },
    /// Remote runner agents (register per environment + dispatch cases to an agent for in-place execution).
    Runner {
        #[command(subcommand)]
        cmd: runner::RunnerCmd,
    },
    /// Native load testing (concurrent load + latency percentile/throughput report; no JMeter).
    Perf {
        #[command(subcommand)]
        cmd: perf::PerfCmd,
    },
    /// Resource pools (execution-node ownership for batch/scenario runs; batch-run/scenario run require --pool).
    Pool {
        #[command(subcommand)]
        cmd: pool::PoolCmd,
    },
    /// Auth / session (login/logout/refresh/me/password + OIDC authorize URL).
    Auth {
        #[command(subcommand)]
        cmd: auth::AuthCmd,
    },
    /// API key management (issue / list / revoke / enable).
    Apikey {
        #[command(subcommand)]
        cmd: apikey::ApikeyCmd,
    },
    /// Per-user LLM model settings (/me/llm-model).
    Llm {
        #[command(subcommand)]
        cmd: llm::LlmCmd,
    },
    /// Notice / inbox (list / read / webhook robots / rules).
    Notice {
        #[command(subcommand)]
        cmd: notice::NoticeCmd,
    },
    /// Follow / unfollow entities (requirement / bug / ...).
    Follow {
        #[command(subcommand)]
        cmd: follow::FollowCmd,
    },
    /// Generic comments on any entity.
    Comment {
        #[command(subcommand)]
        cmd: comment::CommentCmd,
    },
    /// Design proposals (per requirement).
    Proposal {
        #[command(subcommand)]
        cmd: proposal::ProposalCmd,
    },
    /// PRD drafting (generate a requirement skeleton from raw material).
    Prd {
        #[command(subcommand)]
        cmd: prd::PrdCmd,
    },
    /// Import API definitions from a URL, or schedule recurring imports.
    Import {
        #[command(subcommand)]
        cmd: import::ImportCmd,
    },
    /// MCP JSON-RPC passthrough (the server's /mcp tool endpoint).
    Mcp {
        #[command(subcommand)]
        cmd: mcp::McpCmd,
    },
    /// Debug helpers (send a request / list supported protocols).
    Debug {
        #[command(subcommand)]
        cmd: debug::DebugCmd,
    },
    /// API case execution statistics (summary + daily trend).
    Caseexec {
        #[command(subcommand)]
        cmd: caseexec::CaseExecCmd,
    },
    /// Prometheus metrics (plain text; no auth).
    Metrics,
    /// Project files (upload / list / download / attach to module).
    Pfile {
        #[command(subcommand)]
        cmd: pfile::PfileCmd,
    },
}

fn run(cli: Cli) -> R<()> {
    JSON_OUTPUT.store(cli.json, std::sync::atomic::Ordering::Relaxed);
    match cli.cmd {
        Cmd::Login { url, api_key } => root::run_login(url, api_key),
        Cmd::Init { dir, force } => root::run_init(dir, force),
        Cmd::Req { cmd } => req::run(cmd),
        Cmd::Decompose { req, version } => root::run_decompose(req, version),
        Cmd::Decomposition { cmd } => decomposition::run(cmd),
        Cmd::Task { cmd } => task::run(cmd),
        Cmd::Dispatch { decomp, task, title, executor, instructions, project, skills } => {
            root::run_dispatch(decomp, task, title, executor, instructions, project, skills)
        }
        Cmd::Verify { cmd } => verify::run(cmd),
        Cmd::Skill { cmd } => skill::run(cmd),
        Cmd::Agent { cmd } => agent::run(cmd),
        Cmd::Logout => root::run_logout(),
        Cmd::Project { cmd } => project::run(cmd),
        Cmd::Bug { cmd } => bug::run(cmd),
        Cmd::Case { cmd } => case::run(cmd),
        Cmd::Plan { cmd } => plan::run(cmd),
        Cmd::Api { cmd } => api::run(cmd),
        Cmd::User { cmd } => user::run(cmd),
        Cmd::Org { cmd } => org::run(cmd),
        Cmd::Role { cmd } => role::run(cmd),
        Cmd::Apidef { cmd } => apidef::run(cmd),
        Cmd::Scenario { cmd } => scenario::run(cmd),
        Cmd::Env { cmd } => env::run(cmd),
        Cmd::Fcase { cmd } => fcase::run(cmd),
        Cmd::Runner { cmd } => runner::run(cmd),
        Cmd::Perf { cmd } => perf::run(cmd),
        Cmd::Pool { cmd } => pool::run(cmd),
        Cmd::Auth { cmd } => auth::run(cmd),
        Cmd::Apikey { cmd } => apikey::run(cmd),
        Cmd::Llm { cmd } => llm::run(cmd),
        Cmd::Notice { cmd } => notice::run(cmd),
        Cmd::Follow { cmd } => follow::run(cmd),
        Cmd::Comment { cmd } => comment::run(cmd),
        Cmd::Proposal { cmd } => proposal::run(cmd),
        Cmd::Prd { cmd } => prd::run(cmd),
        Cmd::Import { cmd } => import::run(cmd),
        Cmd::Mcp { cmd } => mcp::run(cmd),
        Cmd::Debug { cmd } => debug::run(cmd),
        Cmd::Caseexec { cmd } => caseexec::run(cmd),
        Cmd::Metrics => root::run_metrics(),
        Cmd::Pfile { cmd } => pfile::run(cmd),
    }
}

fn main() {
    if let Err(e) = run(Cli::parse()) {
        eprintln!(" {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use crate::client::{normalize_agent, scaffold_files, Client, Config};

    use super::*;

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
