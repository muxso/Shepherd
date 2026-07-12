use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EventError {
    #[error("event message must not be empty")]
    EmptyMessage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    Decision,
    FileChange,
    TestResult,
    ToolCall,
    Verdict,
    Log,
}

impl EventKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Decision => "DECISION",
            Self::FileChange => "FILE_CHANGE",
            Self::TestResult => "TEST_RESULT",
            Self::ToolCall => "TOOL_CALL",
            Self::Verdict => "VERDICT",
            Self::Log => "LOG",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "DECISION" => Some(Self::Decision),
            "FILE_CHANGE" => Some(Self::FileChange),
            "TEST_RESULT" => Some(Self::TestResult),
            "TOOL_CALL" => Some(Self::ToolCall),
            "VERDICT" => Some(Self::Verdict),
            "LOG" => Some(Self::Log),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewExecutionEvent {
    pub kind: EventKind,
    pub message: String,
    pub detail: Option<String>,
}

impl NewExecutionEvent {
    pub fn new(kind: EventKind, message: &str, detail: Option<&str>) -> Result<Self, EventError> {
        let message = message.trim();
        if message.is_empty() {
            return Err(EventError::EmptyMessage);
        }
        let detail = detail.map(|d| d.trim().to_string()).filter(|d| !d.is_empty());
        Ok(Self { kind, message: message.to_string(), detail })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionEvent {
    pub seq: i64,
    pub kind: EventKind,
    pub message: String,
    pub detail: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_event_validates_and_trims() {
        assert_eq!(
            NewExecutionEvent::new(EventKind::Log, "   ", None).unwrap_err(),
            EventError::EmptyMessage
        );
        let e =
            NewExecutionEvent::new(EventKind::Decision, "  选用 argon2  ", Some("  因为 PHC  "))
                .expect("ok");
        assert_eq!(e.message, "选用 argon2");
        assert_eq!(e.detail.as_deref(), Some("因为 PHC"));
        let e2 = NewExecutionEvent::new(EventKind::Log, "x", Some("  ")).expect("ok");
        assert_eq!(e2.detail, None);
    }

    #[test]
    fn kind_str_roundtrip() {
        for k in [
            EventKind::Decision,
            EventKind::FileChange,
            EventKind::TestResult,
            EventKind::ToolCall,
            EventKind::Verdict,
            EventKind::Log,
        ] {
            assert_eq!(EventKind::parse(k.as_str()), Some(k));
        }
        assert_eq!(EventKind::parse("NOPE"), None);
    }
}
