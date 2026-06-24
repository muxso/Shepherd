//! 执行事件 + 进度回流 sink + claude `stream-json` 解析(stream_events.py 的纯 Rust 移植)。

use async_trait::async_trait;

/// 一条执行事件(审计轨迹)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecEvent {
    pub kind: String,
    pub message: String,
}

impl ExecEvent {
    pub fn new(kind: &str, message: &str) -> Self {
        Self { kind: kind.to_string(), message: message.to_string() }
    }
}

/// 进度回流端口:实现 bridge 回 `/delivery/{id}/events`;design / 测试可用 no-op / 记录。
#[async_trait]
pub trait ProgressSink: Send + Sync {
    async fn emit(&self, ev: ExecEvent);
}

/// 丢弃所有事件(design 模式:提案无事件端点)。
pub struct NoopSink;

#[async_trait]
impl ProgressSink for NoopSink {
    async fn emit(&self, _ev: ExecEvent) {}
}

fn clip(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

/// 把一行 `claude -p --output-format stream-json` 解析成执行事件(可能 0..N 条)。
/// 噪声工具(Grep/Glob/Read)跳过;编辑→FILE_CHANGE,Bash→TOOL_CALL,文本→DECISION。
pub fn parse_claude_line(line: &str) -> Vec<ExecEvent> {
    let line = line.trim();
    if line.is_empty() {
        return vec![];
    }
    let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
        return vec![];
    };
    if v.get("type").and_then(|t| t.as_str()) != Some("assistant") {
        return vec![];
    }
    let Some(content) = v.get("message").and_then(|m| m.get("content")).and_then(|c| c.as_array())
    else {
        return vec![];
    };
    let mut out = Vec::new();
    for c in content {
        match c.get("type").and_then(|t| t.as_str()) {
            Some("tool_use") => {
                let name = c.get("name").and_then(|n| n.as_str()).unwrap_or("");
                let input = c.get("input");
                let ev = match name {
                    "Edit" | "Write" | "MultiEdit" | "NotebookEdit" => {
                        let fp = input
                            .and_then(|i| i.get("file_path"))
                            .and_then(|x| x.as_str())
                            .unwrap_or("");
                        ExecEvent::new("FILE_CHANGE", &format!("{name} {fp}"))
                    }
                    "Bash" => {
                        let cmd = input
                            .and_then(|i| i.get("command"))
                            .and_then(|x| x.as_str())
                            .unwrap_or("");
                        ExecEvent::new("TOOL_CALL", &format!("$ {}", clip(cmd, 120)))
                    }
                    "Grep" | "Glob" | "Read" => continue, // 噪声
                    other => ExecEvent::new("TOOL_CALL", other),
                };
                out.push(ev);
            }
            Some("text") => {
                let t = c.get("text").and_then(|x| x.as_str()).unwrap_or("").trim();
                if !t.is_empty() {
                    out.push(ExecEvent::new("DECISION", &clip(t, 300)));
                }
            }
            _ => {}
        }
    }
    out
}

/// 从 `type=result` 行取最终结果文本与是否出错。
pub fn parse_claude_result(line: &str) -> Option<(bool, String)> {
    let v: serde_json::Value = serde_json::from_str(line.trim()).ok()?;
    if v.get("type").and_then(|t| t.as_str()) != Some("result") {
        return None;
    }
    let is_error = v.get("is_error").and_then(|e| e.as_bool()).unwrap_or(false);
    let text = v
        .get("result")
        .or_else(|| v.get("error"))
        .and_then(|r| r.as_str())
        .unwrap_or("")
        .to_string();
    Some((is_error, text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tool_use_and_text() {
        let edit = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Edit","input":{"file_path":"src/auth.rs"}}]}}"#;
        assert_eq!(parse_claude_line(edit), vec![ExecEvent::new("FILE_CHANGE", "Edit src/auth.rs")]);

        let bash = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Bash","input":{"command":"cargo test"}}]}}"#;
        assert_eq!(parse_claude_line(bash), vec![ExecEvent::new("TOOL_CALL", "$ cargo test")]);

        let text = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"用 argon2 哈希"}]}}"#;
        assert_eq!(parse_claude_line(text), vec![ExecEvent::new("DECISION", "用 argon2 哈希")]);
    }

    #[test]
    fn skips_noise_and_nonjson() {
        let read = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Read","input":{"file_path":"x"}}]}}"#;
        assert!(parse_claude_line(read).is_empty());
        assert!(parse_claude_line("not json").is_empty());
        assert!(parse_claude_line(r#"{"type":"system"}"#).is_empty());
    }

    #[test]
    fn parses_result_line() {
        assert_eq!(
            parse_claude_result(r#"{"type":"result","is_error":false,"result":"done"}"#),
            Some((false, "done".to_string()))
        );
        assert_eq!(
            parse_claude_result(r#"{"type":"result","is_error":true,"error":"boom"}"#),
            Some((true, "boom".to_string()))
        );
        assert_eq!(parse_claude_result(r#"{"type":"assistant"}"#), None);
    }
}
