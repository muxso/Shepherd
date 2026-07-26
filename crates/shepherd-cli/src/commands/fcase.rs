use crate::client::*;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum FcaseCmd {
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

pub fn run(cmd: FcaseCmd) -> R<()> {
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
            let bytes =
                c.get_bytes(&format!("/functional-case/export?projectId={project}"), true)?;
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
    };
    Ok(())
}
