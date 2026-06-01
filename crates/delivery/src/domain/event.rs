//! 执行决策日志(审计轨迹):AI 执行者在一次交付尝试中逐步上报的决策 / 文件变更 / 测试结果 /
//! 工具调用 / 日志。append-only,按 `seq` 单调排序;Shepherd「监管」支柱的落点。

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EventError {
    #[error("event message must not be empty")]
    EmptyMessage,
}

/// 事件类型。覆盖"追踪 AI 的每一步决策、文件变更、测试结果"。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    /// 决策点(为什么这么做)。
    Decision,
    /// 文件变更(新增/修改/删除)。
    FileChange,
    /// 测试结果。
    TestResult,
    /// 工具调用。
    ToolCall,
    /// 普通日志。
    Log,
}

impl EventKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Decision => "DECISION",
            Self::FileChange => "FILE_CHANGE",
            Self::TestResult => "TEST_RESULT",
            Self::ToolCall => "TOOL_CALL",
            Self::Log => "LOG",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "DECISION" => Some(Self::Decision),
            "FILE_CHANGE" => Some(Self::FileChange),
            "TEST_RESULT" => Some(Self::TestResult),
            "TOOL_CALL" => Some(Self::ToolCall),
            "LOG" => Some(Self::Log),
            _ => None,
        }
    }
}

/// 上报一条执行事件的入站请求(尚无 seq)。构造即校验:message 非空。
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

/// 已记录的执行事件。`seq` 由仓储单调分配,用于审计排序。
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
        let e = NewExecutionEvent::new(EventKind::Decision, "  选用 argon2  ", Some("  因为 PHC  ")).expect("ok");
        assert_eq!(e.message, "选用 argon2");
        assert_eq!(e.detail.as_deref(), Some("因为 PHC"));
        // 空白 detail 归一为 None
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
            EventKind::Log,
        ] {
            assert_eq!(EventKind::parse(k.as_str()), Some(k));
        }
        assert_eq!(EventKind::parse("NOPE"), None);
    }
}
