use std::process::Stdio;

use tokio::io::AsyncWriteExt;
use tokio::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub reference: String,
    pub stat: String,
}

async fn git(cwd: &str, args: &[&str]) -> Option<String> {
    let out = Command::new("git").current_dir(cwd).args(args).output().await.ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

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

async fn web_commit_url(cwd: &str, sha: &str) -> Option<String> {
    let remote = git(cwd, &["remote", "get-url", "origin"]).await?;
    let remote = remote.trim().trim_end_matches(".git");
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

pub async fn snapshot(cwd: &str, attempt_id: &str, title: &str) -> Option<Snapshot> {
    let before = git(cwd, &["rev-parse", "HEAD"]).await?;
    let dirty = git(cwd, &["status", "--porcelain"]).await?;
    if dirty.is_empty() {
        return None;
    }
    let stat = git(cwd, &["diff", "--stat", &before]).await.unwrap_or_default();
    git(cwd, &["add", "-A"]).await?;
    let tree = git(cwd, &["write-tree"]).await?;
    let _ = git(cwd, &["reset", "-q"]).await;

    let short = attempt_id.split('-').next().unwrap_or(attempt_id);
    let msg = format!("deliver({short}): {title}\n\nattempt {attempt_id}\n");
    let sha = commit_tree(cwd, &tree, &before, &msg).await?;
    let branch = format!("shepherd/deliver/{short}");
    let _ = git(cwd, &["branch", "-f", &branch, &sha]).await;
    let _ = git(cwd, &["push", "-q", "-f", "origin", &branch]).await;

    let reference =
        web_commit_url(cwd, &sha).await.unwrap_or_else(|| format!("git://{branch}@{sha}"));
    Some(Snapshot { reference, stat })
}

pub async fn add_worktree(base: &str, attempt_id: &str) -> Option<String> {
    let path = std::env::temp_dir().join(format!("shepherd-wt-{attempt_id}"));
    let path = path.to_string_lossy().to_string();
    let _ = git(base, &["worktree", "prune"]).await;
    remove_worktree(base, &path).await;
    git(base, &["worktree", "add", "--detach", &path, "HEAD"]).await?;
    Some(path)
}

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
        assert!(snapshot(&cwd, "att-1", "t").await.is_none());
        std::fs::write(dir.join("a.txt"), "v2 changed").expect("w");
        let snap = snapshot(&cwd, "att-abc-def", "实现登录").await.expect("snapshot");
        assert!(snap.reference.starts_with("git://shepherd/deliver/att@"));
        let branch = git(&cwd, &["rev-parse", "shepherd/deliver/att"]).await;
        assert!(branch.is_some(), "branch created");
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

        let wt = add_worktree(&base, "att-xyz-9").await.expect("worktree");
        std::fs::write(std::path::Path::new(&wt).join("f.txt"), "agent edit").expect("w");
        let snap = snapshot(&wt, "att-xyz-9", "实现").await.expect("snapshot");
        assert!(snap.reference.starts_with("git://shepherd/deliver/att@"));
        assert_eq!(std::fs::read_to_string(dir.join("f.txt")).unwrap(), "base");
        assert!(git(&base, &["rev-parse", "shepherd/deliver/att"]).await.is_some());

        remove_worktree(&base, &wt).await;
        assert!(!std::path::Path::new(&wt).exists(), "worktree dir removed");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
