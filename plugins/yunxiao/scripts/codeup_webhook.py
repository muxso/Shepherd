"""Receive Codeup events and persist work for the internal quality executor."""

import hmac
import json
import os
import re
import sqlite3
import uuid
from contextlib import closing
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


PATH = "/webhook/codeup-events"
CLAIM_PATH = "/internal/quality-queue/claim"
FINISH_PATH = "/internal/quality-queue/finish"
BASELINE_PATH = "/internal/quality-queue/baseline"
LEASE_SECONDS = 1800
MAX_BODY = 1024 * 1024
BRANCH = re.compile(r"[A-Za-z0-9_][A-Za-z0-9_./-]{0,159}")
SHA = re.compile(r"(?:[0-9a-f]{40}|[0-9a-f]{64})")


class InvalidEvent(ValueError):
    pass


def object_field(body, key):
    value = body.get(key, {})
    if not isinstance(value, dict):
        raise InvalidEvent(f"invalid {key}")
    return value


def revision(value):
    value = str(value).lower()
    if not SHA.fullmatch(value) or not value.strip("0"):
        raise InvalidEvent("invalid commit SHA")
    return value


def job_for(event, body, repositories, branch_scan_repositories=(), discover=False):
    if not isinstance(body, dict):
        raise InvalidEvent("expected a JSON object")
    if event == "Push Hook" and body.get("object_kind") == "push":
        repo = str(body.get("project_id") or object_field(body, "project").get("id", ""))
        discovered = repo not in repositories
        if discovered and (not discover or not re.fullmatch(r"[1-9][0-9]{0,19}", repo)):
            raise PermissionError
        ref = body.get("ref")
        if not isinstance(ref, str) or not ref.startswith("refs/heads/") or body.get("checkout_sha", True) is None:
            return None
        branch = ref.removeprefix("refs/heads/")
        default_branch = repositories.get(repo)
        if not BRANCH.fullmatch(branch):
            return None
        if not discovered and branch != default_branch and repo not in branch_scan_repositories:
            if not discover:
                return None
            discovered = True
        after = body.get("after")
        if after in ("0" * 40, "0" * 64):
            return None
        return ("sonar_discovered" if discovered else "sonar", repo,
                branch if branch != default_branch else "", revision(after))
    if event == "Merge Request Hook" and body.get("object_kind") == "merge_request":
        change = object_field(body, "object_attributes")
        repo = str(change.get("target_project_id") or change.get("project_id", ""))
        if repo not in repositories:
            raise PermissionError
        # The default-branch push carries the actual merge or squash result SHA.
        if change.get("action") not in ("open", "update", "reopen") or change.get("state") != "opened":
            return None
        if change.get("work_in_progress") or change.get("draft"):
            return None
        number = str(change.get("local_id") or change.get("iid", ""))
        if not re.fullmatch(r"[1-9][0-9]{0,19}", number):
            raise InvalidEvent("invalid merge request number")
        return ("review", repo, number, revision(object_field(change, "last_commit").get("id")))
    return None


class Queue:
    def __init__(self, path):
        self.path = path
        with closing(sqlite3.connect(self.path)) as db, db:
            db.execute("""
                CREATE TABLE IF NOT EXISTS quality_job (
                    kind TEXT NOT NULL,
                    repository_id TEXT NOT NULL,
                    merge_request TEXT NOT NULL,
                    revision TEXT NOT NULL,
                    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                    PRIMARY KEY (kind, repository_id, merge_request, revision)
                )
            """)

        with closing(sqlite3.connect(self.path)) as db, db:
            db.execute("""
                CREATE TABLE IF NOT EXISTS quality_job_delivery (
                    job_id INTEGER PRIMARY KEY,
                    lease_id TEXT NOT NULL,
                    expires_at INTEGER NOT NULL,
                    completed INTEGER NOT NULL DEFAULT 0
                )
            """)

            db.execute("""
                CREATE TABLE IF NOT EXISTS quality_job_result (
                    job_id INTEGER PRIMARY KEY,
                    result TEXT NOT NULL
                )
            """)

        with closing(sqlite3.connect(self.path)) as db, db:
            db.execute("""CREATE TABLE IF NOT EXISTS quality_push_base (
                job_id INTEGER PRIMARY KEY, base_revision TEXT NOT NULL
            )""")

    def claim(self, kind=None, repository=None):
        with closing(sqlite3.connect(self.path)) as db, db:
            db.execute("BEGIN IMMEDIATE")
            row = db.execute("""
                SELECT j.rowid, j.kind, j.repository_id, j.merge_request, j.revision,
                       b.base_revision, strftime('%Y-%m-%dT%H:%M:%SZ', j.created_at)
                FROM quality_job j LEFT JOIN quality_job_delivery d ON d.job_id=j.rowid
                LEFT JOIN quality_push_base b ON b.job_id=j.rowid
                WHERE (d.job_id IS NULL OR (d.completed=0 AND d.expires_at<=unixepoch()))
                  AND (? IS NULL OR j.kind=?) AND (? IS NULL OR j.repository_id=?)
                ORDER BY j.rowid LIMIT 1
            """, (kind, kind, repository, repository)).fetchone()
            if row is None:
                return None
            job_id, kind, repository, merge_request, sha, base_revision, pushed_at = row
            lease = uuid.uuid4().hex
            db.execute("""
                INSERT INTO quality_job_delivery (job_id, lease_id, expires_at)
                VALUES (?, ?, unixepoch()+?)
                ON CONFLICT(job_id) DO UPDATE SET lease_id=excluded.lease_id,
                                                expires_at=excluded.expires_at
            """, (job_id, lease, LEASE_SECONDS))
            return {"id": job_id, "lease": lease, "kind": kind, "repositoryId": repository,
                    "mergeRequest": (merge_request or None) if kind == "review" else None,
                    "branch": (merge_request or None) if kind in ("sonar", "sonar_discovered") else None, "revision": sha,
                    **({"baseRevision": base_revision, "pushedAt": pushed_at} if base_revision else {})}

    def baseline(self, repository, revision, branch):
        with closing(sqlite3.connect(self.path)) as db:
            row = db.execute("""
                SELECT b.base_revision, strftime('%Y-%m-%dT%H:%M:%SZ', j.created_at)
                FROM quality_job j JOIN quality_push_base b ON b.job_id=j.rowid
                JOIN quality_job_delivery d ON d.job_id=j.rowid
                WHERE j.kind='sonar' AND j.repository_id=? AND j.revision=? AND j.merge_request=?
                  AND d.completed=0 AND d.expires_at>unixepoch()
            """, (repository, revision, branch)).fetchone()
            return {"baseRevision": row[0], "pushedAt": row[1]} if row else {}

    def finish(self, lease, successful, result=None):
        with closing(sqlite3.connect(self.path)) as db, db:
            updated = db.execute("""
                UPDATE quality_job_delivery SET completed=?, expires_at=unixepoch()
                WHERE lease_id=? AND completed=0 AND expires_at>unixepoch()
            """, (int(successful), lease))
            if updated.rowcount != 1:
                return False
            if result is not None:
                db.execute("""
                    INSERT INTO quality_job_result (job_id, result)
                    SELECT job_id, ? FROM quality_job_delivery WHERE lease_id=?
                    ON CONFLICT(job_id) DO UPDATE SET result=excluded.result
                """, (json.dumps(result), lease))
            return True

    def enqueue(self, job, base_revision=None):
        with closing(sqlite3.connect(self.path)) as db, db:
            result = db.execute("""
                INSERT INTO quality_job (kind, repository_id, merge_request, revision)
                VALUES (?, ?, ?, ?) ON CONFLICT DO NOTHING
            """, job)
            if result.rowcount == 1 and base_revision:
                db.execute("INSERT INTO quality_push_base (job_id,base_revision) VALUES (?,?)",
                           (result.lastrowid, base_revision))
            return result.rowcount == 1


class Handler(BaseHTTPRequestHandler):
    def setup(self):
        super().setup()
        self.connection.settimeout(10)

    def respond(self, status, result):
        body = json.dumps(result if isinstance(result, dict) else {"status": result}).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def process_queue(self):
        if not self.server.queue_token or not hmac.compare_digest(
            self.headers.get("Authorization", "").encode(), b"Bearer " + self.server.queue_token
        ):
            return self.respond(401, "unauthorized")
        try:
            length = int(self.headers.get("Content-Length", "0"))
            if not 0 <= length <= 2048:
                return self.respond(400, "invalid_length")
            body = json.loads(self.rfile.read(length)) if length else {}
            if not isinstance(body, dict):
                return self.respond(400, "invalid_request")
            if self.path == CLAIM_PATH:
                kind = body.get("kind")
                repository = body.get("repositoryId")
                if kind is not None and kind not in ("sonar", "sonar_discovered", "review"):
                    return self.respond(400, "invalid_kind")
                if repository is not None and (
                    not isinstance(repository, str) or repository not in self.server.repositories
                ):
                    return self.respond(400, "invalid_repository")
                return self.respond(200, {"job": self.server.queue.claim(kind, repository)})
            if self.path == BASELINE_PATH:
                repository, sha, branch = (body.get("repositoryId"), body.get("revision"), body.get("branch", ""))
                if (not isinstance(repository, str) or repository not in self.server.repositories
                        or not isinstance(sha, str) or not SHA.fullmatch(sha)
                        or not isinstance(branch, str) or (branch and not BRANCH.fullmatch(branch))):
                    return self.respond(400, "invalid_baseline_request")
                return self.respond(200, self.server.queue.baseline(repository, sha, branch))
            if not re.fullmatch(r"[0-9a-f]{32}", str(body.get("lease"))):
                return self.respond(400, "invalid_lease")
            successful = body.get("successful")
            if not isinstance(successful, bool):
                return self.respond(400, "invalid_result")
            result = body.get("result")
            if result is not None and (
                not isinstance(result, dict)
                or result.get("status") not in ("build_failed", "scan_failed", "submitted", "superseded", "repository_unavailable", "branch_not_supported")
            ):
                return self.respond(400, "invalid_result")
            if not self.server.queue.finish(body["lease"], successful, result):
                return self.respond(409, "lease_expired")
            return self.respond(200, "completed" if successful else "released")
        except (ValueError, TimeoutError):
            return self.respond(400, "invalid_request")
        except sqlite3.Error:
            return self.respond(503, "queue_unavailable")

    def do_POST(self):
        if self.path in (CLAIM_PATH, FINISH_PATH, BASELINE_PATH):
            return self.process_queue()
        if self.path != PATH:
            return self.respond(404, "not_found")
        token = self.headers.get("X-Codeup-Token", "").encode()
        if not hmac.compare_digest(token, self.server.token):
            delivery = self.headers.get("X-Codeup-Delivery", "")
            if not re.fullmatch(r"[A-Za-z0-9-]{1,80}", delivery):
                delivery = "unknown"
            self.log_message("Codeup authentication rejected delivery=%s reason=%s",
                             delivery, "token_mismatch" if token else "token_missing")
            return self.respond(401, "unauthorized")
        try:
            length = int(self.headers.get("Content-Length", ""))
            if length <= 0:
                return self.respond(400, "invalid_length")
            if length > MAX_BODY:
                return self.respond(413, "payload_too_large")
            body = json.loads(self.rfile.read(length))
            event = self.headers.get("X-Codeup-Event")
            legacy_event = self.headers.get("Codeup-Event")
            if event and legacy_event and event != legacy_event:
                return self.respond(400, "conflicting_event_headers")
            job = job_for(
                event or legacy_event, body, self.server.repositories, self.server.branch_scan_repositories,
                self.server.discover
            )
            if job is None:
                return self.respond(200, "ignored")
            before = str(body.get("before", "")).lower() if job[0] in ("sonar", "sonar_discovered") else ""
            inserted = self.server.queue.enqueue(job, before if SHA.fullmatch(before) and before.strip("0") else None)
        except (ValueError, TimeoutError):
            return self.respond(400, "invalid_event")
        except PermissionError:
            return self.respond(403, "repository_not_allowed")
        except sqlite3.Error:
            return self.respond(503, "queue_unavailable")
        return self.respond(202 if inserted else 200, "queued" if inserted else "duplicate")


def main():
    token = os.environ["CODEUP_WEBHOOK_TOKEN"].encode()
    repositories = json.loads(os.environ["CODEUP_WEBHOOK_REPOSITORIES"])
    if not token or not isinstance(repositories, dict):
        raise ValueError("configure a webhook token and a repository allowlist")
    if any(not key or not isinstance(branch, str) or not branch for key, branch in repositories.items()):
        raise ValueError("configure a default branch for every repository")
    queue = Queue(os.environ["CODEUP_WEBHOOK_QUEUE"])
    bind = os.environ.get("CODEUP_WEBHOOK_BIND", "127.0.0.1")
    port = int(os.environ.get("CODEUP_WEBHOOK_PORT", "8093"))
    with ThreadingHTTPServer((bind, port), Handler) as server:
        server.token = token
        server.queue_token = os.environ.get("CODEUP_QUALITY_QUEUE_TOKEN", "").encode()
        server.repositories = repositories
        server.branch_scan_repositories = set(filter(None, os.environ.get("CODEUP_BRANCH_SCAN_REPOSITORIES", "").split(",")))
        server.discover = os.environ.get("CODEUP_SONAR_DISCOVER") == "1"
        server.queue = queue
        server.serve_forever()


if __name__ == "__main__":
    main()
