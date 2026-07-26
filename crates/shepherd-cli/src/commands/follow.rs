use crate::client::*;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum FollowCmd {
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

pub fn run(cmd: FollowCmd) -> R<()> {
    let c = Client::new(Config::load())?;
    match cmd {
        FollowCmd::Add { project, entity_type, entity_id } => pretty(&c.post(
            "/follow",
            json!({"projectId": project, "entityType": entity_type, "entityId": entity_id}),
            true,
        )?),
        FollowCmd::Remove { project, entity_type, entity_id } => pretty(&c.delete_body(
            "/follow",
            json!({"projectId": project, "entityType": entity_type, "entityId": entity_id}),
            true,
        )?),
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
    };
    Ok(())
}
