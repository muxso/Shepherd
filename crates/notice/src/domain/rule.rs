//! Notification routing: webhook robots and per-event rules deciding which
//! channels (in-app inbox / robot webhook) an event fans out to.

use crate::domain::NoticeError;

/// Webhook robot platform. Determines the payload shape (and signing for DingTalk).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Feishu,
    Dingtalk,
    Wecom,
}

impl Platform {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_uppercase().as_str() {
            "FEISHU" => Some(Platform::Feishu),
            "DINGTALK" => Some(Platform::Dingtalk),
            "WECOM" => Some(Platform::Wecom),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Platform::Feishu => "FEISHU",
            Platform::Dingtalk => "DINGTALK",
            Platform::Wecom => "WECOM",
        }
    }
}

/// Delivery channel a rule routes to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    InApp,
    Robot,
}

impl Channel {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_uppercase().as_str() {
            "IN_APP" => Some(Channel::InApp),
            "ROBOT" => Some(Channel::Robot),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Channel::InApp => "IN_APP",
            Channel::Robot => "ROBOT",
        }
    }
}

/// A stored webhook robot. created_at is epoch millis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Robot {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub platform: Platform,
    pub webhook_url: String,
    pub secret: String,
    pub enabled: bool,
    pub created_at: i64,
}

/// Robot fields under caller control (everything but id / created_at).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RobotDraft {
    pub project_id: String,
    pub name: String,
    pub platform: Platform,
    pub webhook_url: String,
    pub secret: String,
    pub enabled: bool,
}

impl RobotDraft {
    /// Trims text fields; project/name/webhook must be non-empty.
    pub fn validated(mut self) -> Result<Self, NoticeError> {
        self.project_id = self.project_id.trim().to_string();
        self.name = self.name.trim().to_string();
        self.webhook_url = self.webhook_url.trim().to_string();
        self.secret = self.secret.trim().to_string();
        if self.project_id.is_empty() || self.name.is_empty() || self.webhook_url.is_empty() {
            return Err(NoticeError::InvalidRobot);
        }
        Ok(self)
    }
}

/// A stored notification rule. created_at is epoch millis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    pub id: String,
    pub project_id: String,
    /// Producer event type (BUG_ASSIGNED, MENTIONED, ...) or `*` for all.
    pub event_type: String,
    pub channels: Vec<Channel>,
    pub robot_ids: Vec<String>,
    /// `${title}` / `${operator}` / `${time}` placeholders; empty = default text.
    pub template: String,
    pub enabled: bool,
    pub created_at: i64,
}

/// Rule fields under caller control (everything but id / created_at).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleDraft {
    pub project_id: String,
    pub event_type: String,
    pub channels: Vec<Channel>,
    pub robot_ids: Vec<String>,
    pub template: String,
    pub enabled: bool,
}

impl RuleDraft {
    /// Trims fields and dedups channels/robot ids. An empty channel list is
    /// legal: it mutes the event entirely.
    pub fn validated(mut self) -> Result<Self, NoticeError> {
        self.project_id = self.project_id.trim().to_string();
        self.event_type = self.event_type.trim().to_string();
        if self.project_id.is_empty() || self.event_type.is_empty() {
            return Err(NoticeError::InvalidRule);
        }
        let mut channels: Vec<Channel> = Vec::new();
        for c in self.channels {
            if !channels.contains(&c) {
                channels.push(c);
            }
        }
        self.channels = channels;
        let mut ids: Vec<String> = Vec::new();
        for r in self.robot_ids {
            let r = r.trim().to_string();
            if !r.is_empty() && !ids.contains(&r) {
                ids.push(r);
            }
        }
        self.robot_ids = ids;
        Ok(self)
    }
}

/// Renders a rule template against an event; a blank template falls back to a
/// default "title / operator / time" text.
pub fn render_template(template: &str, title: &str, operator: &str, time: &str) -> String {
    let template = template.trim();
    if template.is_empty() {
        return format!("【Shepherd】{title}\n操作人: {operator}\n时间: {time}");
    }
    template.replace("${title}", title).replace("${operator}", operator).replace("${time}", time)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_and_channel_roundtrip() {
        for p in [Platform::Feishu, Platform::Dingtalk, Platform::Wecom] {
            assert_eq!(Platform::parse(p.as_str()), Some(p));
        }
        assert_eq!(Platform::parse("feishu"), Some(Platform::Feishu));
        assert_eq!(Platform::parse("slack"), None);
        for c in [Channel::InApp, Channel::Robot] {
            assert_eq!(Channel::parse(c.as_str()), Some(c));
        }
        assert_eq!(Channel::parse("in_app"), Some(Channel::InApp));
        assert_eq!(Channel::parse("EMAIL"), None);
    }

    #[test]
    fn robot_draft_trims_and_rejects_blanks() {
        let d = RobotDraft {
            project_id: " p1 ".into(),
            name: " bot ".into(),
            platform: Platform::Wecom,
            webhook_url: " https://x ".into(),
            secret: " s ".into(),
            enabled: true,
        };
        let d = d.validated().expect("valid");
        assert_eq!((d.project_id.as_str(), d.name.as_str()), ("p1", "bot"));
        assert_eq!(d.webhook_url, "https://x");
        assert_eq!(d.secret, "s");

        let bad = RobotDraft {
            project_id: "p1".into(),
            name: "bot".into(),
            platform: Platform::Feishu,
            webhook_url: "  ".into(),
            secret: String::new(),
            enabled: true,
        };
        assert_eq!(bad.validated(), Err(NoticeError::InvalidRobot));
    }

    #[test]
    fn rule_draft_dedups_and_requires_event() {
        let d = RuleDraft {
            project_id: "p1".into(),
            event_type: " BUG_ASSIGNED ".into(),
            channels: vec![Channel::InApp, Channel::InApp, Channel::Robot],
            robot_ids: vec!["r1".into(), " r1 ".into(), "".into(), "r2".into()],
            template: String::new(),
            enabled: true,
        };
        let d = d.validated().expect("valid");
        assert_eq!(d.event_type, "BUG_ASSIGNED");
        assert_eq!(d.channels, vec![Channel::InApp, Channel::Robot]);
        assert_eq!(d.robot_ids, vec!["r1", "r2"]);

        let bad = RuleDraft {
            project_id: "p1".into(),
            event_type: " ".into(),
            channels: vec![],
            robot_ids: vec![],
            template: String::new(),
            enabled: true,
        };
        assert_eq!(bad.validated(), Err(NoticeError::InvalidRule));
    }

    #[test]
    fn render_template_substitutes_or_falls_back() {
        let s = render_template("${title} by ${operator} @ ${time}", "T", "admin", "2026");
        assert_eq!(s, "T by admin @ 2026");
        let s = render_template("  ", "T", "admin", "2026");
        assert!(s.contains("T") && s.contains("admin") && s.contains("2026"));
    }
}
