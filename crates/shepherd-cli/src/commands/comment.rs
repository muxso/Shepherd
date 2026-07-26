use crate::client::*;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum CommentCmd {
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

pub fn run(cmd: CommentCmd) -> R<()> {
    let c = Client::new(Config::load())?;
    match cmd {
        CommentCmd::Add { target_type, target_id, content } => pretty(&c.post(
            "/comment",
            json!({"targetType": target_type, "targetId": target_id, "content": content}),
            true,
        )?),
        CommentCmd::List { target_type, target_id } => pretty(
            &c.get(&format!("/comment?targetType={target_type}&targetId={target_id}"), true)?,
        ),
        CommentCmd::Delete { id } => {
            c.delete(&format!("/comment/{id}"), true)?;
            println!(" 已删除评论 {id}");
        }
    };
    Ok(())
}
