//! CLI 执行后端。claude(流式 stream-json)、codex/opencode(通用,粗粒度)、mock(测试)。
//!
//! 后端职责:spawn CLI 跑 prompt,边跑边经 sink 回流进度事件,返回**最终文本输出**。
//! git 快照 / 回调由 runtime 主流程按模式(implement / design)编排,后端不关心。
//!
//! 健壮性(对齐 CI runner 对子进程的处理):
//! - **每任务超时**:卡死的 CLI 不能一直占着并发槽 —— 到点连同其派生子进程整组 SIGKILL。
//! - **进程组隔离**:`process_group(0)` 起新组,杀的是整组,claude 再 spawn 的 bash 等不留孤儿。
//! - **stderr 并发抽干**:流式读 stdout 的同时必须持续读 stderr,否则 stderr 写满 64KB
//!   管道缓冲 → 子进程阻塞 → 死锁。`kill_on_drop` 兜底进程在 future 被丢弃时也回收。

use std::process::Stdio;
use std::time::Duration;

use async_trait::async_trait;
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};

use crate::events::{parse_claude_line, parse_claude_result, ExecEvent, ProgressSink};

/// 单条 stderr 保留上限(只留头部用于报错;超出部分照常读出丢弃以免写端阻塞)。
const STDERR_CAP: usize = 64 * 1024;

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

/// 让子进程成为新进程组组长(pgid == pid),便于超时/关停时整组回收。
fn in_own_process_group(cmd: &mut Command) {
    #[cfg(unix)]
    {
        cmd.process_group(0);
    }
    #[cfg(not(unix))]
    {
        let _ = cmd;
    }
}

/// 给整组发 SIGKILL(以 `process_group(0)` 起组,故 pgid == pid;负号即整组)。
fn kill_process_group(pid: u32) {
    #[cfg(unix)]
    unsafe {
        libc::kill(-(pid as i32), libc::SIGKILL);
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
    }
}

/// 持续读完 `r`(避免管道写端阻塞),但只保留前 `cap` 字节作报错用。
async fn drain_capped<R: AsyncReadExt + Unpin>(mut r: R, cap: usize) -> String {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        match r.read(&mut chunk).await {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                if buf.len() < cap {
                    let take = n.min(cap - buf.len());
                    buf.extend_from_slice(&chunk[..take]);
                }
            }
        }
    }
    String::from_utf8_lossy(&buf).trim().to_string()
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
    pub timeout: Duration,
}

impl ClaudeBackend {
    pub fn new(timeout: Duration) -> Self {
        Self { bin: std::env::var("CLAUDE_BIN").unwrap_or_else(|_| "claude".into()), timeout }
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
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        in_own_process_group(&mut cmd);
        let mut child = spawn_retrying_etxtbsy(&mut cmd)
            .await
            .map_err(|e| BackendError::Run(format!("spawn {}: {e}", self.bin)))?;
        let pid = child.id();

        // stdin:并发写,免得大 prompt 写入与 stdout 读取相互阻塞。
        if let Some(mut stdin) = child.stdin.take() {
            let bytes = prompt.as_bytes().to_vec();
            tokio::spawn(async move {
                let _ = stdin.write_all(&bytes).await; // drop → EOF
            });
        }
        // stderr:并发抽干(封顶),防止写满管道缓冲拖死 stdout 流。
        let stderr_task = child
            .stderr
            .take()
            .map(|e| tokio::spawn(async move { drain_capped(e, STDERR_CAP).await }));
        let stdout = child.stdout.take().ok_or_else(|| BackendError::Run("no stdout".into()))?;

        // 读流 + 等退出,整体加超时。
        let run = async {
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
            Ok::<_, BackendError>((result, status))
        };

        let (result, status) = match tokio::time::timeout(self.timeout, run).await {
            Ok(r) => r?,
            Err(_) => {
                if let Some(pid) = pid {
                    kill_process_group(pid);
                }
                sink.emit(ExecEvent::new(
                    "TIMEOUT",
                    &format!("超时 {}s,已终止", self.timeout.as_secs()),
                ))
                .await;
                return Err(BackendError::Run(format!(
                    "claude timeout after {}s",
                    self.timeout.as_secs()
                )));
            }
        };

        let err = match stderr_task {
            Some(t) => t.await.unwrap_or_default(),
            None => String::new(),
        };
        if !status.success() {
            return Err(BackendError::Run(format!("claude exited {status}: {err}")));
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
    pub timeout: Duration,
}

impl GenericCliBackend {
    pub fn codex(timeout: Duration) -> Self {
        let cmd = std::env::var("CODEX_CMD").unwrap_or_else(|_| "codex exec".into());
        Self { name: "codex", cmd: cmd.split_whitespace().map(String::from).collect(), timeout }
    }
    pub fn opencode(timeout: Duration) -> Self {
        let cmd = std::env::var("OPENCODE_CMD").unwrap_or_else(|_| "opencode run".into());
        Self { name: "opencode", cmd: cmd.split_whitespace().map(String::from).collect(), timeout }
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
        let mut cmd = Command::new(program);
        cmd.current_dir(cwd)
            .args(args)
            .arg(prompt)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        in_own_process_group(&mut cmd);
        let child = cmd.spawn().map_err(|e| BackendError::Run(format!("spawn {program}: {e}")))?;
        let pid = child.id();
        let out = match tokio::time::timeout(self.timeout, child.wait_with_output()).await {
            Ok(r) => r.map_err(|e| BackendError::Run(e.to_string()))?,
            Err(_) => {
                if let Some(pid) = pid {
                    kill_process_group(pid);
                }
                return Err(BackendError::Run(format!(
                    "{} timeout after {}s",
                    self.name,
                    self.timeout.as_secs()
                )));
            }
        };
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

    const TEST_TIMEOUT: Duration = Duration::from_secs(30);

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

    /// 写一个可执行的伪 CLI 脚本到临时文件,返回路径。
    fn write_script(tag: &str, body: &str) -> std::path::PathBuf {
        use std::io::Write;
        use std::os::unix::fs::PermissionsExt;
        let path = std::env::temp_dir().join(format!("ar-fake-{tag}-{}.sh", std::process::id()));
        let mut f = std::fs::File::create(&path).expect("create");
        write!(f, "{body}").expect("write");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        path
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
            timeout: TEST_TIMEOUT,
        };
        let sink = RecSink::default();
        let out = b.execute("the prompt", ".", &sink).await.expect("run");
        assert_eq!(out, "## codex 设计");
        assert_eq!(sink.events.lock().unwrap()[0].kind, "DECISION");
    }

    #[tokio::test]
    async fn claude_backend_streams_events_and_returns_result() {
        // 伪 claude 脚本:忽略参数,吃掉 stdin,吐两行 stream-json(Edit 工具调用 + result)。
        let path = write_script(
            "claude",
            "#!/bin/sh\ncat >/dev/null\nprintf '%s\\n' '{\"type\":\"assistant\",\"message\":{\"content\":[{\"type\":\"tool_use\",\"name\":\"Edit\",\"input\":{\"file_path\":\"a.rs\"}}]}}' '{\"type\":\"result\",\"is_error\":false,\"result\":\"完成\"}'\n",
        );
        let b = ClaudeBackend { bin: path.to_string_lossy().to_string(), timeout: TEST_TIMEOUT };
        let sink = RecSink::default();
        let out = b.execute("do it", ".", &sink).await.expect("run");
        assert_eq!(out, "完成");
        let evs = sink.events.lock().unwrap();
        assert_eq!(evs.len(), 1);
        assert_eq!(evs[0], ExecEvent::new("FILE_CHANGE", "Edit a.rs"));
        let _ = std::fs::remove_file(&path);
    }

    /// 回归:claude 先吐 ~100KB stderr 再吐 stdout result —— 若 stderr 不并发抽干,
    /// 子进程会在写满管道缓冲(64KB)处阻塞,而我们在等 stdout → 死锁。
    #[tokio::test]
    async fn claude_backend_drains_large_stderr_without_deadlock() {
        let path = write_script(
            "claude-stderr",
            "#!/bin/sh\ncat >/dev/null\ni=0\nwhile [ $i -lt 1200 ]; do printf 'noise-%04d-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\\n' $i >&2; i=$((i+1)); done\nprintf '%s\\n' '{\"type\":\"result\",\"is_error\":false,\"result\":\"ok\"}'\n",
        );
        let b = ClaudeBackend { bin: path.to_string_lossy().to_string(), timeout: TEST_TIMEOUT };
        let sink = RecSink::default();
        let out = b.execute("do it", ".", &sink).await.expect("must not deadlock");
        assert_eq!(out, "ok");
        let _ = std::fs::remove_file(&path);
    }

    /// 超时即终止:卡住的 CLI 不该一直占槽 —— 应在 ~timeout 内返回 Err。
    #[tokio::test]
    async fn claude_backend_times_out_and_kills() {
        let path = write_script("claude-hang", "#!/bin/sh\ncat >/dev/null\nsleep 30\n");
        let b = ClaudeBackend {
            bin: path.to_string_lossy().to_string(),
            timeout: Duration::from_millis(500),
        };
        let sink = RecSink::default();
        let start = std::time::Instant::now();
        let err = b.execute("do it", ".", &sink).await.expect_err("should time out");
        assert!(err.to_string().contains("timeout"), "got: {err}");
        assert!(start.elapsed() < Duration::from_secs(10), "returned promptly on timeout");
        let _ = std::fs::remove_file(&path);
    }
}
