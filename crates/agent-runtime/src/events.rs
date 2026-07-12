use async_trait::async_trait;

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

#[async_trait]
pub trait ProgressSink: Send + Sync {
    async fn emit(&self, ev: ExecEvent);
}

pub struct NoopSink;

#[async_trait]
impl ProgressSink for NoopSink {
    async fn emit(&self, _ev: ExecEvent) {}
}

fn clip(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

pub fn parse_claude_line(line: &str) -> Vec<ExecEvent> {
    let line = line.trim();
    if line.is_empty() {
        return vec![];
    }
    let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
        return vec![];
    };
    match v.get("type").and_then(|t| t.as_str()) {
        Some("assistant") => parse_assistant(&v),
        Some("user") => parse_tool_results(&v),
        _ => vec![],
    }
}

fn parse_assistant(v: &serde_json::Value) -> Vec<ExecEvent> {
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
                        if is_test_command(cmd) {
                            ExecEvent::new("TEST_RESULT", &format!("运行测试: {}", clip(cmd, 120)))
                        } else {
                            ExecEvent::new("TOOL_CALL", &format!("$ {}", clip(cmd, 120)))
                        }
                    }
                    "Grep" | "Glob" | "Read" => continue,
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

/// Extracts test summaries from tool_result (stream-json user messages) to capture real pass/fail.
fn parse_tool_results(v: &serde_json::Value) -> Vec<ExecEvent> {
    let Some(content) = v.get("message").and_then(|m| m.get("content")).and_then(|c| c.as_array())
    else {
        return vec![];
    };
    let mut out = Vec::new();
    for b in content {
        if b.get("type").and_then(|t| t.as_str()) != Some("tool_result") {
            continue;
        }
        if let Some(summary) = test_summary(&tool_result_text(b.get("content"))) {
            out.push(ExecEvent::new("TEST_RESULT", &summary));
        }
    }
    out
}

/// tool_result.content may be a string or a `[{type:text,text}]` block array.
fn tool_result_text(content: Option<&serde_json::Value>) -> String {
    match content {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Array(blocks)) => blocks
            .iter()
            .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

fn is_test_command(cmd: &str) -> bool {
    let c = cmd.to_lowercase();
    [
        "cargo test",
        "cargo nextest",
        "npm test",
        "npm run test",
        "pnpm test",
        "yarn test",
        "pytest",
        "go test",
        "jest",
        "vitest",
        "mvn test",
        "gradle test",
        "phpunit",
    ]
    .iter()
    .any(|p| c.contains(p))
}

/// Best-effort match of test-summary lines across runners (cargo / pytest / jest / mocha…).
fn test_summary(out: &str) -> Option<String> {
    for line in out.lines() {
        let l = line.trim();
        let low = l.to_lowercase();
        let hit = low.starts_with("test result:")
            || low.starts_with("tests:")
            || (low.contains("passed") && (low.contains("failed") || low.contains("error")))
            || low.contains(" passing")
            || low.contains(" failing");
        if hit {
            return Some(clip(l, 200));
        }
    }
    None
}

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
        assert_eq!(
            parse_claude_line(edit),
            vec![ExecEvent::new("FILE_CHANGE", "Edit src/auth.rs")]
        );

        let bash = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Bash","input":{"command":"ls -la"}}]}}"#;
        assert_eq!(parse_claude_line(bash), vec![ExecEvent::new("TOOL_CALL", "$ ls -la")]);

        let text = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"用 argon2 哈希"}]}}"#;
        assert_eq!(parse_claude_line(text), vec![ExecEvent::new("DECISION", "用 argon2 哈希")]);
    }

    #[test]
    fn test_command_is_classified_as_test_result() {
        let bash = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Bash","input":{"command":"cargo test -p auth"}}]}}"#;
        assert_eq!(
            parse_claude_line(bash),
            vec![ExecEvent::new("TEST_RESULT", "运行测试: cargo test -p auth")]
        );
    }

    #[test]
    fn tool_result_test_summary_becomes_test_result() {
        // String content (cargo summary embedded in multi-line output).
        let cargo = r#"{"type":"user","message":{"content":[{"type":"tool_result","content":"running 3 tests\n...\ntest result: ok. 3 passed; 0 failed; 0 ignored"}]}}"#;
        assert_eq!(
            parse_claude_line(cargo),
            vec![ExecEvent::new("TEST_RESULT", "test result: ok. 3 passed; 0 failed; 0 ignored")]
        );
        // Block-array content (pytest style).
        let pytest = r#"{"type":"user","message":{"content":[{"type":"tool_result","content":[{"type":"text","text":"=== 2 passed, 1 failed in 0.3s ==="}]}]}}"#;
        let evs = parse_claude_line(pytest);
        assert_eq!(evs.len(), 1);
        assert_eq!(evs[0].kind, "TEST_RESULT");
        assert!(evs[0].message.contains("2 passed, 1 failed"));
        // Non-test output produces no event.
        let noise = r#"{"type":"user","message":{"content":[{"type":"tool_result","content":"hello world"}]}}"#;
        assert!(parse_claude_line(noise).is_empty());
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
