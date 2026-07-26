use crate::client::*;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum ReqCmd {
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

pub fn run(cmd: ReqCmd) -> R<()> {
    let c = Client::new(Config::load())?;
    match cmd {
        ReqCmd::Add {
            project,
            title,
            description,
            criteria,
        } => pretty(&c.post(
            "/requirement",
            json!({"projectId": project, "title": title, "description": description, "acceptanceCriteria": criteria}),
            true,
        )?),
        ReqCmd::List {
            project,
            page,
            page_size,
        } => pretty(&c.get(
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
    };
    Ok(())
}
