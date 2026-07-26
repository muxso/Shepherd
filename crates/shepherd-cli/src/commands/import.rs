use crate::client::*;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum ImportCmd {
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

pub fn run(cmd: ImportCmd) -> R<()> {
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
    };
    Ok(())
}
