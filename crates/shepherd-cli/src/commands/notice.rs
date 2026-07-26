use crate::client::*;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum NoticeCmd {
    /// List notices (optional project / category / tab filters).
    List {
        #[arg(long)]
        project: Option<String>,
        /// PLAN | BUG | CASE | API | SCHEDULE.
        #[arg(long)]
        category: Option<String>,
        /// all | at | unread | read (default all).
        #[arg(long)]
        tab: Option<String>,
        #[arg(long, default_value_t = 1)]
        page: u32,
        #[arg(long = "page-size", default_value_t = 50)]
        page_size: u32,
    },
    /// Unread count.
    UnreadCount {
        #[arg(long)]
        project: Option<String>,
    },
    /// Mark all as read.
    ReadAll {
        #[arg(long)]
        project: Option<String>,
    },
    /// Mark one notice as read.
    Read {
        #[arg(long)]
        id: String,
    },
    /// Webhook robots (Feishu / DingTalk / WeCom).
    Robots {
        #[command(subcommand)]
        cmd: NoticeRobotCmd,
    },
    /// Notification rules.
    Rules {
        #[command(subcommand)]
        cmd: NoticeRuleCmd,
    },
}

#[derive(Subcommand)]
pub enum NoticeRobotCmd {
    /// List robots.
    List,
    /// Create a robot.
    Create {
        #[arg(long)]
        project: String,
        #[arg(long)]
        name: String,
        /// FEISHU | DINGTALK | WECOM.
        #[arg(long)]
        platform: String,
        #[arg(long = "webhook-url")]
        webhook_url: String,
        #[arg(long)]
        secret: Option<String>,
        #[arg(long, default_value_t = true)]
        enable: bool,
    },
    /// Update a robot.
    Update {
        #[arg(long)]
        id: String,
        #[arg(long)]
        project: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        platform: String,
        #[arg(long = "webhook-url")]
        webhook_url: String,
        #[arg(long)]
        secret: Option<String>,
        #[arg(long, default_value_t = true)]
        enable: bool,
    },
    /// Delete a robot.
    Delete {
        #[arg(long)]
        id: String,
    },
    /// Send a test message to the robot's webhook.
    Test {
        #[arg(long)]
        id: String,
    },
}

#[derive(Subcommand)]
pub enum NoticeRuleCmd {
    /// List rules (optionally by project).
    List {
        #[arg(long)]
        project: Option<String>,
    },
    /// Create a rule.
    Create {
        #[arg(long)]
        project: String,
        #[arg(long)]
        event_type: String,
        /// Channels, comma-separated (IN_APP, ROBOT).
        #[arg(long, value_delimiter = ',')]
        channels: Vec<String>,
        #[arg(long, value_delimiter = ',')]
        robot_ids: Vec<String>,
        #[arg(long, default_value = "")]
        template: String,
        #[arg(long, default_value_t = true)]
        enable: bool,
    },
    /// Update a rule.
    Update {
        #[arg(long)]
        id: String,
        #[arg(long)]
        project: String,
        #[arg(long)]
        event_type: String,
        /// Channels, comma-separated (IN_APP, ROBOT).
        #[arg(long, value_delimiter = ',')]
        channels: Vec<String>,
        #[arg(long, value_delimiter = ',')]
        robot_ids: Vec<String>,
        #[arg(long, default_value = "")]
        template: String,
        #[arg(long, default_value_t = true)]
        enable: bool,
    },
    /// Delete a rule.
    Delete {
        #[arg(long)]
        id: String,
    },
}

pub fn run(cmd: NoticeCmd) -> R<()> {
    let c = Client::new(Config::load())?;
    match cmd {
        NoticeCmd::List {
            project,
            category,
            tab,
            page,
            page_size,
        } => {
            let mut parts: Vec<String> = Vec::new();
            if let Some(p) = &project {
                parts.push(format!("projectId={p}"));
            }
            if let Some(c) = &category {
                parts.push(format!("category={c}"));
            }
            if let Some(t) = &tab {
                parts.push(format!("tab={t}"));
            }
            parts.push(format!("page={page}"));
            parts.push(format!("pageSize={page_size}"));
            pretty(&c.get(&format!("/notice?{}", parts.join("&")), true)?);
        }
        NoticeCmd::UnreadCount { project } => {
            let path = match &project {
                Some(p) => format!("/notice/unread-count?projectId={p}"),
                None => "/notice/unread-count".to_string(),
            };
            pretty(&c.get(&path, true)?);
        }
        NoticeCmd::ReadAll { project } => {
            let path = match &project {
                Some(p) => format!("/notice/read-all?projectId={p}"),
                None => "/notice/read-all".to_string(),
            };
            c.post(&path, json!({}), true)?;
            println!(" 已全部已读");
        }
        NoticeCmd::Read { id } => {
            c.post(&format!("/notice/{id}/read"), json!({}), true)?;
            println!(" 已标记已读 {id}");
        }
        NoticeCmd::Robots { cmd } => match cmd {
            NoticeRobotCmd::List => pretty(&c.get("/notice/robots", true)?),
            NoticeRobotCmd::Create {
                project,
                name,
                platform,
                webhook_url,
                secret,
                enable,
            } => {
                let mut body =
                    json!({"projectId": project, "name": name, "platform": platform, "webhookUrl": webhook_url, "enabled": enable});
                if let Some(s) = secret {
                    body["secret"] = json!(s);
                }
                pretty(&c.post("/notice/robots", body, true)?);
            }
            NoticeRobotCmd::Update {
                id,
                project,
                name,
                platform,
                webhook_url,
                secret,
                enable,
            } => {
                let mut body =
                    json!({"projectId": project, "name": name, "platform": platform, "webhookUrl": webhook_url, "enabled": enable});
                if let Some(s) = secret {
                    body["secret"] = json!(s);
                }
                pretty(&c.put(&format!("/notice/robots/{id}"), body, true)?);
            }
            NoticeRobotCmd::Delete { id } => {
                c.delete(&format!("/notice/robots/{id}"), true)?;
                println!(" 已删除 robot {id}");
            }
            NoticeRobotCmd::Test { id } => {
                pretty(&c.post(&format!("/notice/robots/{id}/test"), json!({}), true)?);
            }
        },
        NoticeCmd::Rules { cmd } => match cmd {
            NoticeRuleCmd::List { project } => {
                let path = match &project {
                    Some(p) => format!("/notice/rules?projectId={p}"),
                    None => "/notice/rules".to_string(),
                };
                pretty(&c.get(&path, true)?);
            }
            NoticeRuleCmd::Create {
                project,
                event_type,
                channels,
                robot_ids,
                template,
                enable,
            } => pretty(&c.post(
                "/notice/rules",
                json!({"projectId": project, "eventType": event_type, "channels": channels, "robotIds": robot_ids, "template": template, "enabled": enable}),
                true,
            )?),
            NoticeRuleCmd::Update {
                id,
                project,
                event_type,
                channels,
                robot_ids,
                template,
                enable,
            } => pretty(&c.put(
                &format!("/notice/rules/{id}"),
                json!({"projectId": project, "eventType": event_type, "channels": channels, "robotIds": robot_ids, "template": template, "enabled": enable}),
                true,
            )?),
            NoticeRuleCmd::Delete { id } => {
                c.delete(&format!("/notice/rules/{id}"), true)?;
                println!(" 已删除 rule {id}");
            }
        },
    };
    Ok(())
}
