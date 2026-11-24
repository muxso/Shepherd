import hashlib
import io
import json
import os
import runpy
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from codeup_sonar_worker import discovered_repository, fetch_checkout, push_baseline, scan, scan_discovered
from codeup_webhook import Queue, job_for


SHA = "a" * 40


class DiscoveryTest(unittest.TestCase):
    def test_configured_baseline_matches_active_repository_branch_and_revision(self):
        with tempfile.TemporaryDirectory() as directory:
            queue = Queue(Path(directory) / "queue.db")
            before = "b" * 40
            queue.enqueue(("sonar", "123", "feature/test", SHA), before)
            self.assertEqual(queue.baseline("123", SHA, "feature/test"), {})
            job = queue.claim("sonar", "123")
            self.assertEqual(queue.baseline("123", SHA, "feature/test")["baseRevision"], before)
            self.assertEqual(queue.baseline("123", SHA, ""), {})
            self.assertEqual(queue.baseline("456", SHA, "feature/test"), {})
            queue.finish(job["lease"], True)
            self.assertEqual(queue.baseline("123", SHA, "feature/test"), {})

    def test_sonar_permission_failure_preserves_git_metadata(self):
        job = {"repositoryId": "123", "revision": SHA, "baseRevision": "b" * 40, "pushedAt": "2026-11-21T00:00:00Z"}
        with patch("codeup_sonar_worker.run", side_effect=[None, "Test Author\ttest@example.org\n"]), \
             patch("codeup_sonar_worker.sonar_request", side_effect=[
                 {"analyses": [{"revision": "b" * 40, "key": "base-analysis"}]}, PermissionError("Forbidden"),
             ]):
            metadata = push_baseline(job, Path("."), {}, "https://sonar.invalid", "token", "named-project", "main", "main")
        self.assertIn("-Dsonar.analysis.pushBaselineVerified=false", metadata)
        self.assertIn("-Dsonar.analysis.pushAuthor=Test Author", metadata)

    def test_other_repository_enters_discovery_queue_once(self):
        event = {"object_kind": "push", "project_id": 123, "ref": "refs/heads/main", "after": SHA}
        with self.assertRaises(PermissionError):
            job_for("Push Hook", event, {})
        task = job_for("Push Hook", event, {}, discover=True)
        with tempfile.TemporaryDirectory() as directory:
            queue = Queue(Path(directory) / "queue.db")
            self.assertTrue(queue.enqueue(task))
            self.assertFalse(queue.enqueue(task))
            self.assertIsNone(queue.claim("sonar"))
            claimed = queue.claim("sonar_discovered")
            self.assertEqual((claimed["repositoryId"], claimed["branch"], claimed["revision"]),
                             ("123", "main", SHA))
        self.assertIsNone(job_for("Push Hook", dict(event, after="0" * 40), {}, discover=True))
        self.assertEqual(job_for("Push Hook", event, {"123": "main"}, discover=True)[0], "sonar")

    def test_configured_feature_branch_can_fall_back_to_discovery(self):
        event = {"object_kind": "push", "project_id": 123, "ref": "refs/heads/feature/test", "after": SHA}
        self.assertIsNone(job_for("Push Hook", event, {"123": "main"}))
        self.assertEqual(job_for("Push Hook", event, {"123": "main"}, discover=True),
                         ("sonar_discovered", "123", "feature/test", SHA))
        self.assertEqual(job_for("Push Hook", event, {"123": "main"}, {"123"}, discover=True)[0], "sonar")

    def test_discovered_branch_gets_its_own_archivable_project(self):
        branch = "feature/test"
        expected = "codeup:123:branch:" + hashlib.sha256(branch.encode()).hexdigest()[:16]
        with tempfile.TemporaryDirectory() as directory:
            token = Path(directory) / "token"
            token.write_text("test")
            with patch.dict(os.environ, {"CODEUP_SONAR_TOKEN_FILE": str(token), "SONAR_HOST_URL": "https://sonar.invalid", "SONAR_BRANCH_ANALYSIS": "0"}), \
                 patch("codeup_sonar_worker.discovered_repository", return_value=("git@codeup.aliyun.com:org/repo.git", "main")), \
                 patch("codeup_sonar_worker.scan") as scanner:
                scan_discovered("123", SHA, branch)
                self.assertEqual(scanner.call_args.args[3], expected)
                self.assertTrue(scanner.call_args.kwargs["isolated_branch"])
        push_report = runpy.run_path(str(Path(__file__).resolve().parents[1] / "deploy/sonar/wecom_notify.py"))["push_report"]
        event = {"status": "SUCCESS", "project": {"key": expected}, "revision": SHA,
                 "taskId": "task", "analysedAt": "2026-11-21T00:00:00+0000",
                 "properties": {"sonar.analysis.repositoryId": "123", "sonar.analysis.gitBranch": branch}}
        self.assertEqual(push_report(event, {})["repositoryId"], "123")
        event["properties"]["sonar.analysis.repositoryId"] = "456"
        self.assertIsNone(push_report(event, {}))

    def test_clone_address_comes_from_codeup_metadata_in_the_configured_organization(self):
        with tempfile.TemporaryDirectory() as directory:
            token = Path(directory) / "token"
            token.write_text("test-token")
            with patch.dict(os.environ, {
                "CODEUP_SONAR_ORGANIZATION": "org",
                "CODEUP_SONAR_CODEUP_TOKEN_FILE": str(token),
            }), patch("codeup_sonar_worker.urlopen") as request:
                metadata = {"id": 123, "pathWithNamespace": "org/group/repo", "defaultBranch": "develop"}
                request.return_value = io.BytesIO(json.dumps(metadata).encode())
                self.assertEqual(discovered_repository("123"), ("git@codeup.aliyun.com:org/group/repo.git", "develop"))
                self.assertTrue(request.call_args.args[0].full_url.endswith("/org/repositories/123"))
                metadata["pathWithNamespace"] = "another-org/group/repo"
                request.return_value = io.BytesIO(json.dumps(metadata).encode())
                self.assertIsNone(discovered_repository("123"))

    def test_fetch_cache_refreshes_branch_and_keeps_commit_history(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            repo = root / "source"
            repo.mkdir()

            def git(*args, cwd=repo):
                return subprocess.check_output(["git", *args], cwd=cwd, text=True).strip()

            git("init", "--quiet", "--initial-branch=main")
            git("config", "user.email", "test@example.org")
            git("config", "user.name", "Test Author")
            git("commit", "--quiet", "--allow-empty", "-m", "first")
            first = git("rev-parse", "HEAD")
            env = dict(os.environ, CODEUP_GIT_CACHE_DIR=str(root / "cache"))
            cold = root / "cold"
            ref = fetch_checkout(str(repo), "main", cold, env)
            self.assertEqual(git("rev-parse", ref, cwd=cold), first)
            git("commit", "--quiet", "--allow-empty", "-m", "second")
            warm = root / "warm"
            ref = fetch_checkout(str(repo), "main", warm, env)
            self.assertEqual(git("rev-parse", ref, cwd=warm), git("rev-parse", "HEAD"))
            git("merge-base", "--is-ancestor", first, ref, cwd=warm)
            self.assertEqual(len(list((root / "cache").glob("*.git"))), 1)

    def test_branch_analysis_reuses_repository_project_key(self):
        job = {"revision": SHA, "branch": "feature/test"}
        args = (job, "git@codeup.aliyun.com:org/repo.git", "main", "codeup:123", "cli", "https://sonar.invalid")
        with tempfile.TemporaryDirectory() as directory:
            token = Path(directory) / "token"
            token.write_text("test-token")
            self.assertEqual(scan(*args, token)["status"], "branch_not_supported")

            def run(command, *args, **kwargs):
                if command[:2] == ["git", "rev-parse"]:
                    return SHA
                if command[:2] == ["git", "show"]:
                    return "Test Author"
                if command[0] == "sonar-scanner":
                    self.assertIn("-Dsonar.projectKey=codeup:123", command)
                    self.assertIn("-Dsonar.projectName=repo", command)
                    self.assertIn("-Dsonar.branch.name=feature/test", command)
                    prefix = "-Dsonar.scanner.metadataFilePath="
                    metadata = next(arg.removeprefix(prefix) for arg in command if arg.startswith(prefix))
                    Path(metadata).write_text("ceTaskId=test-task\n")

            with patch("codeup_sonar_worker.run", side_effect=run), patch("codeup_sonar_worker.ensure_sonar_project") as provision:
                result = scan(*args, token, use_repository_settings=False, branch_analysis=True, project_name="repo")
                provision.assert_called_once_with("https://sonar.invalid", "test-token", "codeup:123", "repo")
                self.assertEqual(result["taskId"], "test-task")


if __name__ == "__main__":
    unittest.main()
