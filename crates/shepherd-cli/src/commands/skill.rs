use crate::client::*;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum SkillCmd {
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

pub fn run(cmd: SkillCmd) -> R<()> {
    let c = Client::new(Config::load())?;
    match cmd {
        SkillCmd::Add {
            project,
            name,
            instructions,
            includes,
        } => pretty(&c.post(
            "/skill",
            json!({"projectId": project, "name": name, "instructions": instructions, "includes": includes}),
            true,
        )?),
        SkillCmd::Compose { project, ids } => pretty(&c.post(
            "/skill/compose",
            json!({"projectId": project, "skillIds": ids}),
            true,
        )?),
    };
    Ok(())
}
