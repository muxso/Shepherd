//! CLI 执行后端。claude(流式 stream-json)、codex/opencode(通用,粗粒度)、mock(测试)。
//!
//! 后端职责:spawn CLI 跑 prompt,边跑边经 sink 回流进度事件,返回**最终文本输出**。
//! git 快照 / 回调由 runtime 主流程按模式(implement / design)编排,后端不关心。

use std::process::Stdio;
use std::time::Duration;

use async_trait::async_trait;
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};

use crate::events::{parse_claude_line, parse_claude_result, ExecEvent, ProgressSink};

#[derive(Debug, Error)]
pub enum BackendError {
    #[error("backend error: {0}")]
    Run(String),
}

/// 刚写出的可执行文件偶发 `ETXTBSY`(os error 26):多线程进程里另一线程在
/// fork→exec 的窗口内继承了该文件的写句柄,内核认为它仍可写而拒绝 exec。
/// 短退避重试即可消除(测试里现写现跑会触发;fleet runtime 现写包装脚本同理)。
async fn spawn_retrying_etxtbsy(cmd: &mut Command) -> std::io::Result<Child> {
    for _ in 0..4 {
        match cmd.spawn() {
            Err(e) if e.raw_os_error() == Some(26) => {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            other => return other,
        }
    }
    cmd.spawn() // 末次:无论成败都透传
}

#[async_trait]
pub trait CliAgentBackend: Send + Sync {
    fn cli_name(&self) -> &str;
    /// 在 `cwd`(每任务隔离的 worktree)里跑 prompt;运行中经 sink 回流事件,返回最终文本。
    async fn execute(
        &self,
        prompt: &str,
        cwd: &str,
        sink: &dyn ProgressSink,
    ) -> Result<String, BackendError>;
}

// ───────────────────────── Claude(流式)─────────────────────────

/// `claude -p --output-format stream-json`:逐行解析,实时回流事件,取 result 文本。
pub struct ClaudeBackend {
    pub bin: String,
}

impl Default for ClaudeBackend {
    fn default() -> Self {
        Self { bin: std::env::var("CLAUDE_BIN").unwrap_or_else(|_| "claude".into()) }
    }
}

#[async_trait]
impl CliAgentBackend for ClaudeBackend {
    fn cli_name(&self) -> &str {
        "claude"
    }

    async fn execute(
        &self,
        prompt: &str,
        cwd: &str,
        sink: &dyn ProgressSink,
    ) -> Result<String, BackendError> {
        let mut cmd = Command::new(&self.bin);
        cmd.current_dir(cwd)
            .args([
                "-p",
                "--output-format",
                "stream-json",
                "--verbose",
                "--permission-mode",
                "acceptEdits",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = spawn_retrying_etxtbsy(&mut cmd)
            .await
            .map_err(|e| BackendError::Run(format!("spawn {}: {e}", self.bin)))?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(prompt.as_bytes()).await.map_err(|e| BackendError::Run(e.to_string()))?;
            // stdin drop → EOF
        }
        let stdout = child.stdout.take().ok_or_else(|| BackendError::Run("no stdout".into()))?;
        let mut lines = BufReader::new(stdout).lines();
        let mut result: Option<(bool, String)> = None;
        while let Some(line) =
            lines.next_line().await.map_err(|e| BackendError::Run(e.to_string()))?
        {
            for ev in parse_claude_line(&line) {
                sink.emit(ev).await;
            }
            if let Some(r) = parse_claude_result(&line) {
                result = Some(r);
            }
        }
        let status = child.wait().await.map_err(|e| BackendError::Run(e.to_string()))?;
        if !status.success() {
            let mut err = String::new();
            if let Some(mut e) = child.stderr.take() {
                let _ = e.read_to_string(&mut err).await;
            }
            return Err(BackendError::Run(format!("claude exited {status}: {}", err.trim())));
        }
        match result {
            Some((true, text)) => Err(BackendError::Run(format!("claude error: {text}"))),
            Some((false, text)) => Ok(text),
            None => Ok(String::new()),
        }
    }
}

// ───────────────────────── 通用(codex / opencode)─────────────────────────

/// 非流式:跑 `cmd... "<prompt>"`,捕获 stdout 作输出。CLI 命令可经 env 覆盖。
pub struct GenericCliBackend {
    pub name: &'static str,
    pub cmd: Vec<String>,
}

impl GenericCliBackend {
    pub fn codex() -> Self {
        let cmd = std::env::var("CODEX_CMD").unwrap_or_else(|_| "codex exec".into());
        Self { name: "codex", cmd: cmd.split_whitespace().map(String::from).collect() }
    }
    pub fn opencode() -> Self {
        let cmd = std::env::var("OPENCODE_CMD").unwrap_or_else(|_| "opencode run".into());
        Self { name: "opencode", cmd: cmd.split_whitespace().map(String::from).collect() }
    }
}

#[async_trait]
impl CliAgentBackend for GenericCliBackend {
    fn cli_name(&self) -> &str {
        self.name
    }

    async fn execute(
        &self,
        prompt: &str,
        cwd: &str,
        sink: &dyn ProgressSink,
    ) -> Result<String, BackendError> {
        let (program, args) =
            self.cmd.split_first().ok_or_else(|| BackendError::Run("empty cmd".into()))?;
        sink.emit(ExecEvent::new("DECISION", &format!("调用 {} 执行任务", self.name))).await;
        let out = Command::new(program)
            .current_dir(cwd)
            .args(args)
            .arg(prompt)
            .output()
            .await
            .map_err(|e| BackendError::Run(format!("spawn {program}: {e}")))?;
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr);
            return Err(BackendError::Run(format!(
                "{} exited {}: {}",
                self.name,
                out.status,
                err.trim()
            )));
        }
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }
}

// ───────────────────────── Mock(测试/演示)─────────────────────────

pub struct MockBackend {
    pub output: String,
}

impl Default for MockBackend {
    fn default() -> Self {
        Self { output: "## Mock 输出\n(agent-runtime mock backend)".to_string() }
    }
}

#[async_trait]
impl CliAgentBackend for MockBackend {
    fn cli_name(&self) -> &str {
        "mock"
    }

    async fn execute(
        &self,
        _prompt: &str,
        _cwd: &str,
        sink: &dyn ProgressSink,
    ) -> Result<String, BackendError> {
        sink.emit(ExecEvent::new("DECISION", "mock backend 运行中")).await;
        Ok(self.output.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct RecSink {
        events: Mutex<Vec<ExecEvent>>,
    }
    #[async_trait]
    impl ProgressSink for RecSink {
        async fn emit(&self, ev: ExecEvent) {
            self.events.lock().unwrap().push(ev);
        }
    }

    #[tokio::test]
    async fn mock_emits_and_returns() {
        let b = MockBackend { output: "DOC".into() };
        let sink = RecSink::default();
        assert_eq!(b.execute("p", ".", &sink).await.expect("run"), "DOC");
        assert_eq!(sink.events.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn generic_backend_runs_cmd_and_captures_stdout() {
        // 用 /bin/sh -c 充当 CLI:吃掉 prompt 参数,打印固定输出。
        let b = GenericCliBackend {
            name: "codex",
            cmd: vec!["/bin/sh".into(), "-c".into(), "printf '## codex 设计'".into(), "_".into()],
        };
        let sink = RecSink::default();
        let out = b.execute("the prompt", ".", &sink).await.expect("run");
        assert_eq!(out, "## codex 设计");
        assert_eq!(sink.events.lock().unwrap()[0].kind, "DECISION");
    }

    #[tokio::test]
    async fn claude_backend_streams_events_and_returns_result() {
        use std::io::Write;
        use std::os::unix::fs::PermissionsExt;
        // 伪 claude 脚本:忽略 -p/--output-format 等参数,吃掉 stdin(prompt),
        // 吐两行 stream-json(一个 Edit 工具调用 + 一个 result)。
        let path = std::env::temp_dir().join(format!("ar-fake-claude-{}.sh", std::process::id()));
        {
            let mut f = std::fs::File::create(&path).expect("create");
            writeln!(
                f,
                "#!/bin/sh\ncat >/dev/null\nprintf '%s\\n' '{{\"type\":\"assistant\",\"message\":{{\"content\":[{{\"type\":\"tool_use\",\"name\":\"Edit\",\"input\":{{\"file_path\":\"a.rs\"}}}}]}}}}' '{{\"type\":\"result\",\"is_error\":false,\"result\":\"完成\"}}'"
            )
            .expect("write");
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        }
        let b = ClaudeBackend { bin: path.to_string_lossy().to_string() };
        let sink = RecSink::default();
        let out = b.execute("do it", ".", &sink).await.expect("run");
        assert_eq!(out, "完成");
        let evs = sink.events.lock().unwrap();
        assert_eq!(evs.len(), 1);
        assert_eq!(evs[0], ExecEvent::new("FILE_CHANGE", "Edit a.rs"));
        let _ = std::fs::remove_file(&path);
    }
}
