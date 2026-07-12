use std::process::Stdio;
use std::time::Duration;

use async_trait::async_trait;
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};

use crate::events::{parse_claude_line, parse_claude_result, ExecEvent, ProgressSink};

const STDERR_CAP: usize = 64 * 1024;

#[derive(Debug, Error)]
pub enum BackendError {
    #[error("backend error: {0}")]
    Run(String),
}

// ETXTBSY (os error 26): a just-written executable can be transiently busy when
// another thread inherited its write handle across fork→exec; short backoff-retry clears it.
async fn spawn_retrying_etxtbsy(cmd: &mut Command) -> std::io::Result<Child> {
    for _ in 0..4 {
        match cmd.spawn() {
            Err(e) if e.raw_os_error() == Some(26) => {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            other => return other,
        }
    }
    cmd.spawn()
}

// On Windows, npm-installed CLIs (claude/codebuddy/opencode) are `<name>.cmd` shims,
// but std resolves bare names as `.exe` only, so spawn fails with NotFound. When a bare
// name has no exe on PATH but a same-named .cmd exists, use the shim's full path
// (std runs .cmd via cmd.exe). Native .exe binaries like codex, and explicit configs
// with a path/extension (CLAUDE_BIN etc.), are unaffected.
fn resolve_cli_program(name: &str) -> String {
    if !cfg!(windows) || name.contains(['.', '/', '\\']) {
        return name.to_string();
    }
    find_cmd_shim(name, std::env::var_os("PATH").as_deref()).unwrap_or_else(|| name.to_string())
}

// Walk PATH for the first dir providing `<name>.exe` or `<name>.cmd`:
// exe hit → None (let std resolve normally); cmd hit → Some(full shim path).
fn find_cmd_shim(name: &str, path: Option<&std::ffi::OsStr>) -> Option<String> {
    for dir in std::env::split_paths(path?) {
        if dir.join(format!("{name}.exe")).is_file() {
            return None;
        }
        let shim = dir.join(format!("{name}.cmd"));
        if shim.is_file() {
            return Some(shim.to_string_lossy().to_string());
        }
    }
    None
}

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

// Kill the whole process group (pgid == pid via process_group(0); negative pid = group)
// so a spawned CLI that forks children leaves no orphans on timeout/shutdown.
// Windows has no process-group semantics: kill_on_drop (TerminateProcess) covers the
// direct child, but grandchildren may survive (no Job Object).
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
    async fn execute(
        &self,
        prompt: &str,
        cwd: &str,
        sink: &dyn ProgressSink,
    ) -> Result<String, BackendError>;
}

pub struct ClaudeBackend {
    pub bin: String,
    pub timeout: Duration,
}

impl ClaudeBackend {
    pub fn new(timeout: Duration) -> Self {
        let bin = std::env::var("CLAUDE_BIN").unwrap_or_else(|_| "claude".into());
        Self { bin: resolve_cli_program(&bin), timeout }
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

        if let Some(mut stdin) = child.stdin.take() {
            let bytes = prompt.as_bytes().to_vec();
            tokio::spawn(async move {
                let _ = stdin.write_all(&bytes).await;
            });
        }
        // Drain stderr concurrently: if it fills the 64KB pipe buffer while we're
        // blocked reading stdout, the child stalls on write → deadlock.
        let stderr_task = child
            .stderr
            .take()
            .map(|e| tokio::spawn(async move { drain_capped(e, STDERR_CAP).await }));
        let stdout = child.stdout.take().ok_or_else(|| BackendError::Run("no stdout".into()))?;

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

pub struct GenericCliBackend {
    pub name: &'static str,
    pub cmd: Vec<String>,
    pub timeout: Duration,
}

impl GenericCliBackend {
    fn from_env(name: &'static str, env_key: &str, default_cmd: &str, timeout: Duration) -> Self {
        let cmd = std::env::var(env_key).unwrap_or_else(|_| default_cmd.into());
        let mut cmd: Vec<String> = cmd.split_whitespace().map(String::from).collect();
        if let Some(program) = cmd.first_mut() {
            *program = resolve_cli_program(program);
        }
        Self { name, cmd, timeout }
    }
    pub fn codex(timeout: Duration) -> Self {
        Self::from_env("codex", "CODEX_CMD", "codex exec", timeout)
    }
    pub fn opencode(timeout: Duration) -> Self {
        Self::from_env("opencode", "OPENCODE_CMD", "opencode run", timeout)
    }
    pub fn codebuddy(timeout: Duration) -> Self {
        // Default mirrors ClaudeBackend's permission policy: allow edits when
        // non-interactive, otherwise the CLI refuses to write files.
        Self::from_env(
            "codebuddy",
            "CODEBUDDY_CMD",
            "codebuddy -p --permission-mode acceptEdits",
            timeout,
        )
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
            self.events.lock().unwrap_or_else(std::sync::PoisonError::into_inner).push(ev);
        }
    }

    fn write_script(tag: &str, body: &str) -> std::path::PathBuf {
        use std::io::Write;
        use std::os::unix::fs::PermissionsExt;
        let path = std::env::temp_dir().join(format!("ar-fake-{tag}-{}.sh", std::process::id()));
        let mut f = std::fs::File::create(&path).expect("create");
        write!(f, "{body}").expect("write");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        path
    }

    #[test]
    fn cmd_shim_lookup_prefers_exe_and_falls_back_to_cmd() {
        let root = std::env::temp_dir().join(format!("ar-shim-{}", std::process::id()));
        let (d1, d2) = (root.join("one"), root.join("two"));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&d1).expect("mkdir");
        std::fs::create_dir_all(&d2).expect("mkdir");
        std::fs::write(d1.join("tool.cmd"), "").expect("w");
        std::fs::write(d1.join("native.exe"), "").expect("w");
        std::fs::write(d2.join("native.cmd"), "").expect("w");
        let path = std::env::join_paths([&d1, &d2]).expect("join");

        let shim = find_cmd_shim("tool", Some(&path)).expect("shim");
        assert!(shim.ends_with(if cfg!(windows) { "one\\tool.cmd" } else { "one/tool.cmd" }));
        // An exe in the same dir wins: std resolves it; later dirs' .cmd is ignored.
        assert!(find_cmd_shim("native", Some(&path)).is_none());
        assert!(find_cmd_shim("missing", Some(&path)).is_none());
        assert!(find_cmd_shim("tool", None).is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn mock_emits_and_returns() {
        let b = MockBackend { output: "DOC".into() };
        let sink = RecSink::default();
        assert_eq!(b.execute("p", ".", &sink).await.expect("run"), "DOC");
        assert_eq!(sink.events.lock().unwrap_or_else(std::sync::PoisonError::into_inner).len(), 1);
    }

    #[tokio::test]
    async fn generic_backend_runs_cmd_and_captures_stdout() {
        let b = GenericCliBackend {
            name: "codex",
            cmd: vec!["/bin/sh".into(), "-c".into(), "printf '## codex 设计'".into(), "_".into()],
            timeout: TEST_TIMEOUT,
        };
        let sink = RecSink::default();
        let out = b.execute("the prompt", ".", &sink).await.expect("run");
        assert_eq!(out, "## codex 设计");
        assert_eq!(
            sink.events.lock().unwrap_or_else(std::sync::PoisonError::into_inner)[0].kind,
            "DECISION"
        );
    }

    #[tokio::test]
    async fn claude_backend_streams_events_and_returns_result() {
        let path = write_script(
            "claude",
            "#!/bin/sh\ncat >/dev/null\nprintf '%s\\n' '{\"type\":\"assistant\",\"message\":{\"content\":[{\"type\":\"tool_use\",\"name\":\"Edit\",\"input\":{\"file_path\":\"a.rs\"}}]}}' '{\"type\":\"result\",\"is_error\":false,\"result\":\"完成\"}'\n",
        );
        let b = ClaudeBackend { bin: path.to_string_lossy().to_string(), timeout: TEST_TIMEOUT };
        let sink = RecSink::default();
        let out = b.execute("do it", ".", &sink).await.expect("run");
        assert_eq!(out, "完成");
        let evs = sink.events.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        assert_eq!(evs.len(), 1);
        assert_eq!(evs[0], ExecEvent::new("FILE_CHANGE", "Edit a.rs"));
        let _ = std::fs::remove_file(&path);
    }

    // Regression: large stderr before stdout result must not deadlock (stderr drain).
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
