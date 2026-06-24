//! 交付物 = 真实代码变动:把工作区改动快照成一个 commit 推到 per-attempt 分支。
//! 用 `git` 底层命令(write-tree/commit-tree),不切分支、不动工作区 —— 经 `tokio::process`
//! 编排(非 shell 脚本)。push 尽力而为;失败回退本地 git:// 引用。

use std::process::Stdio;

use tokio::io::AsyncWriteExt;
use tokio::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    /// 交付物引用:网页 commit URL,或 `git://branch@sha`。
    pub reference: String,
    /// `git diff --stat` 概要。
    pub stat: String,
}

/// 跑一条 git 命令,成功返回 trim 后的 stdout,失败返回 None。
async fn git(cwd: &str, args: &[&str]) -> Option<String> {
    let out = Command::new("git").current_dir(cwd).args(args).output().await.ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// `git commit-tree <tree> -p <parent>`,message 经 stdin。
async fn commit_tree(cwd: &str, tree: &str, parent: &str, message: &str) -> Option<String> {
    let mut child = Command::new("git")
        .current_dir(cwd)
        .args(["commit-tree", tree, "-p", parent])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .ok()?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(message.as_bytes()).await.ok()?;
    }
    let out = child.wait_with_output().await.ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// 由 origin remote + sha 拼网页 commit URL(github/gitlab 兼容)。
async fn web_commit_url(cwd: &str, sha: &str) -> Option<String> {
    let remote = git(cwd, &["remote", "get-url", "origin"]).await?;
    let remote = remote.trim().trim_end_matches(".git");
    // 形如 git@host:path 或 https://host/path
    let (host, path) = if let Some(rest) = remote.strip_prefix("git@") {
        let (h, p) = rest.split_once(':')?;
        (h.to_string(), p.to_string())
    } else if let Some(rest) = remote.strip_prefix("https://").or_else(|| remote.strip_prefix("http://")) {
        let (h, p) = rest.split_once('/')?;
        (h.to_string(), p.to_string())
    } else {
        return None;
    };
    let seg = if host.contains("gitlab") { "/-/commit/" } else { "/commit/" };
    Some(format!("https://{host}/{path}{seg}{sha}"))
}

/// 快照工作区改动为 commit。无改动 → None。
pub async fn snapshot(cwd: &str, attempt_id: &str, title: &str) -> Option<Snapshot> {
    let before = git(cwd, &["rev-parse", "HEAD"]).await?;
    let dirty = git(cwd, &["status", "--porcelain"]).await?;
    if dirty.is_empty() {
        return None;
    }
    let stat = git(cwd, &["diff", "--stat", &before]).await.unwrap_or_default();
    git(cwd, &["add", "-A"]).await?;
    let tree = git(cwd, &["write-tree"]).await?;
    let _ = git(cwd, &["reset", "-q"]).await; // 还原暂存区,工作区不变

    let short = attempt_id.split('-').next().unwrap_or(attempt_id);
    let msg = format!("deliver({short}): {title}\n\nattempt {attempt_id}\n");
    let sha = commit_tree(cwd, &tree, &before, &msg).await?;
    let branch = format!("shepherd/deliver/{short}");
    let _ = git(cwd, &["branch", "-f", &branch, &sha]).await;
    let _ = git(cwd, &["push", "-q", "-f", "origin", &branch]).await; // 尽力而为

    let reference =
        web_commit_url(cwd, &sha).await.unwrap_or_else(|| format!("git://{branch}@{sha}"));
    Some(Snapshot { reference, stat })
}

/// 为某任务建一个独立 git worktree(从 base HEAD detached 检出),返回其路径。
/// 隔离:每任务自己的工作副本 → 并发实现互不踩、base 工作区不受影响。失败 → None。
pub async fn add_worktree(base: &str, attempt_id: &str) -> Option<String> {
    // 用完整 attempt_id 保证并发唯一(同进程多任务也不撞)。
    let path = std::env::temp_dir().join(format!("shepherd-wt-{attempt_id}"));
    let path = path.to_string_lossy().to_string();
    let _ = git(base, &["worktree", "prune"]).await;
    remove_worktree(base, &path).await; // 清可能的残留
    git(base, &["worktree", "add", "--detach", &path, "HEAD"]).await?;
    Some(path)
}

/// 移除 worktree(目录 + 登记);分支/commit 在共享对象库里,保留。
pub async fn remove_worktree(base: &str, path: &str) {
    let _ = git(base, &["worktree", "remove", "--force", path]).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn run(cwd: &str, args: &[&str]) {
        Command::new("git").current_dir(cwd).args(args).output().await.expect("git");
    }

    #[tokio::test]
    async fn snapshots_dirty_worktree_into_commit() {
        let dir = std::env::temp_dir().join(format!("ar-git-{}", std::process::id()));
        let cwd = dir.to_string_lossy().to_string();
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        run(&cwd, &["init", "-q"]).await;
        run(&cwd, &["config", "user.email", "t@t"]).await;
        run(&cwd, &["config", "user.name", "t"]).await;
        std::fs::write(dir.join("a.txt"), "v1").expect("w");
        run(&cwd, &["add", "-A"]).await;
        run(&cwd, &["commit", "-q", "-m", "init"]).await;
        // 干净 → None
        assert!(snapshot(&cwd, "att-1", "t").await.is_none());
        // 改动 → Some,且分支已建
        std::fs::write(dir.join("a.txt"), "v2 changed").expect("w");
        let snap = snapshot(&cwd, "att-abc-def", "实现登录").await.expect("snapshot");
        assert!(snap.reference.starts_with("git://shepherd/deliver/att@"));
        let branch = git(&cwd, &["rev-parse", "shepherd/deliver/att"]).await;
        assert!(branch.is_some(), "branch created");
        // 工作区保持改动(未被 reset 丢失)
        assert_eq!(std::fs::read_to_string(dir.join("a.txt")).unwrap(), "v2 changed");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn worktree_isolates_changes_from_base() {
        let dir = std::env::temp_dir().join(format!("ar-wt-base-{}", std::process::id()));
        let base = dir.to_string_lossy().to_string();
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        run(&base, &["init", "-q"]).await;
        run(&base, &["config", "user.email", "t@t"]).await;
        run(&base, &["config", "user.name", "t"]).await;
        std::fs::write(dir.join("f.txt"), "base").expect("w");
        run(&base, &["add", "-A"]).await;
        run(&base, &["commit", "-q", "-m", "init"]).await;

        // 建 worktree,只在 worktree 里改文件 → 快照成 commit。
        let wt = add_worktree(&base, "att-xyz-9").await.expect("worktree");
        std::fs::write(std::path::Path::new(&wt).join("f.txt"), "agent edit").expect("w");
        let snap = snapshot(&wt, "att-xyz-9", "实现").await.expect("snapshot");
        assert!(snap.reference.starts_with("git://shepherd/deliver/att@"));
        // base 工作区**未被污染**(仍是 base)。
        assert_eq!(std::fs::read_to_string(dir.join("f.txt")).unwrap(), "base");
        // 分支在共享库里可见。
        assert!(git(&base, &["rev-parse", "shepherd/deliver/att"]).await.is_some());

        remove_worktree(&base, &wt).await;
        assert!(!std::path::Path::new(&wt).exists(), "worktree dir removed");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
