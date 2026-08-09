use crate::client::*;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum ApidefCmd {
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

pub fn run(cmd: ApidefCmd) -> R<()> {
    let c = Client::new(Config::load())?;
    match cmd {
        ApidefCmd::Create {
            project,
            name,
            protocol,
            method,
            path,
        } => pretty(&c.post(
            "/api/definition",
            json!({"projectId": project, "name": name, "protocol": protocol, "method": method, "path": path}),
            true,
        )?),
        ApidefCmd::Import { project, file, url } => {
            let raw = match (url, file) {
                (Some(u), _) => c.fetch_text(&u)?,
                (None, Some(f)) => std::fs::read_to_string(&f)?,
                (None, None) => return Err("either --file or --url must be specified".into()),
            };
            let content: Value = serde_json::from_str(&raw)
                .map_err(|e| format!("import content is not valid JSON: {e}"))?;
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
        ApidefCmd::Case {
            def,
            name,
            method,
            url,
            body,
            expect_status,
        } => pretty(&c.post(
            &format!("/api/definition/{def}/case"),
            json!({"name": name, "method": method, "url": url, "body": body, "assertions": status_assertions(expect_status)}),
            true,
        )?),
        ApidefCmd::Cases { def } => {
            pretty(&c.get(&format!("/api/definition/{def}/case"), true)?)
        }
        ApidefCmd::CaseNew {
            project,
            def,
            name,
            method,
            url,
            body,
            expect_status,
        } => pretty(&c.post(
            "/api/case",
            json!({"projectId": project, "apiDefinitionId": def, "name": name, "method": method, "url": url, "body": body, "assertions": status_assertions(expect_status)}),
            true,
        )?),
        ApidefCmd::GenSuite {
            project,
            base,
            no_scenario,
        } => {
            gen_suite(&c, &project, base.trim_end_matches('/'), no_scenario)?;
        }
        ApidefCmd::CaseList {
            project,
            current,
            page_size,
        } => pretty(&c.get(
            &format!("/api/case?projectId={project}&current={current}&pageSize={page_size}"),
            true,
        )?),
        ApidefCmd::CaseExec {
            case,
            current,
            page_size,
        } => pretty(&c.get(
            &format!("/api/case/{case}/executions?current={current}&pageSize={page_size}"),
            true,
        )?),
        ApidefCmd::CaseRun {
            case,
            project,
            run_mode,
            pool,
            env,
        } => pretty(&c.post(
            &format!("/api/case/{case}/run"),
            json!({"projectId": project, "runMode": run_mode, "poolId": pool, "environmentId": env}),
            true,
        )?),
        ApidefCmd::Mock {
            def,
            name,
            response_status,
            body,
        } => pretty(&c.post(
            &format!("/api/definition/{def}/mock"),
            json!({"name": name, "responseStatus": response_status, "responseBody": body}),
            true,
        )?),
        ApidefCmd::Mocks { def } => {
            pretty(&c.get(&format!("/api/definition/{def}/mock"), true)?)
        }
        ApidefCmd::Delete { id } => {
            c.delete(&format!("/api/definition/{id}"), true)?;
            println!(" deleted API definition {id}");
        }
        ApidefCmd::References { id } => {
            pretty(&c.get(&format!("/api/definition/{id}/references"), true)?)
        }
        ApidefCmd::Spec { id, spec_json } => {
            let spec: Value = serde_json::from_str(&spec_json)
                .map_err(|e| format!("--spec-json is not valid JSON: {e}"))?;
            c.put(&format!("/api/definition/{id}/spec"), json!({ "spec": spec }), true)?;
            println!(" updated spec of API definition {id}");
        }
        ApidefCmd::Module { id, module_id, unset } => {
            let mid = if unset { None } else { module_id };
            c.put(
                &format!("/api/definition/{id}/module"),
                json!({ "moduleId": mid }),
                true,
            )?;
            println!(
                " API definition {id} {}",
                if mid.is_some() { "moved into module" } else { "moved out to uncategorized" }
            );
        }
        ApidefCmd::Status { id, status } => {
            c.put(
                &format!("/api/definition/{id}/status"),
                json!({ "status": status }),
                true,
            )?;
            println!(" set API definition {id} status to {status}");
        }
        ApidefCmd::Changes { id } => {
            pretty(&c.get(&format!("/api/definition/{id}/changes"), true)?)
        }
    };
    Ok(())
}
