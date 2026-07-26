use crate::client::*;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum EnvCmd {
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

pub fn run(cmd: EnvCmd) -> R<()> {
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
        EnvCmd::Delete { id } => pretty(&c.delete(&format!("/api/environment/{id}"), true)?),
    };
    Ok(())
}
