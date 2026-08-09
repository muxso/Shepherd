use crate::client::*;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum ScenarioCmd {
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

pub fn run(cmd: ScenarioCmd) -> R<()> {
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
        ScenarioCmd::Step {
            scenario,
            kind,
            ref_mode,
            order,
            ref_id,
            method,
            url,
            body,
            expect_status,
            assertions_json,
            control_json,
        } => {
            let mut step = json!({"kind": kind.to_uppercase(), "refMode": ref_mode.to_uppercase(), "order": order});
            if let Some(r) = ref_id {
                step["refId"] = json!(r);
            }
            if let Some(m) = method {
                let assertions = match assertions_json {
                    Some(aj) => serde_json::from_str(&aj)
                        .map_err(|e| format!("--assertions-json is not valid JSON: {e}"))?,
                    None => status_assertions(expect_status),
                };
                step["request"] = json!({"method": m, "url": url.unwrap_or_default(), "body": body, "assertions": assertions});
            }
            if let Some(cj) = control_json {
                step["control"] = serde_json::from_str(&cj)
                    .map_err(|e| format!("--control-json is not valid JSON: {e}"))?;
            }
            pretty(&c.post(&format!("/api/scenario/{scenario}/step"), step, true)?)
        }
        ScenarioCmd::Compile { id } => {
            pretty(&c.get(&format!("/api/scenario/{id}/compile"), true)?)
        }
        ScenarioCmd::Executions {
            id,
            current,
            page_size,
        } => pretty(&c.get(
            &format!("/api/scenario/{id}/executions?current={current}&pageSize={page_size}"),
            true,
        )?),
        ScenarioCmd::Run {
            id,
            project,
            run_mode,
            pool,
            env,
        } => pretty(&c.post(
            &format!("/api/scenario/{id}/run"),
            json!({"projectId": project, "runMode": run_mode, "poolId": pool, "environmentId": env}),
            true,
        )?),
    };
    Ok(())
}
