#!/usr/bin/env python3
"""Scan Codeup pushes for configured or API-discovered repositories."""

import fcntl
import hashlib
import json
import os
import re
import subprocess
import sys
import time
from pathlib import Path
from tempfile import TemporaryDirectory
from urllib.error import HTTPError
from urllib.parse import quote, urlencode
from urllib.request import Request, urlopen


def queue_request(path, body):
    url = os.environ["CODEUP_QUALITY_QUEUE_URL"].rstrip("/")
    token_file = os.environ.get("CODEUP_QUALITY_QUEUE_TOKEN_FILE")
    token = Path(token_file).read_text().strip() if token_file else os.environ["CODEUP_QUALITY_QUEUE_TOKEN"]
    request = Request(
        f"{url}/internal/quality-queue/{path}",
        data=json.dumps(body).encode(),
        headers={"Authorization": f"Bearer {token}", "Content-Type": "application/json"},
    )
    with urlopen(request, timeout=10) as response:
        return json.load(response)


def ensure_sonar_project(url, token, key, name):
    def request(path, data=None):
        req = Request(
            f"{url}{path}",
            data=urlencode(data).encode() if data else None,
            headers={"Authorization": f"Bearer {token}"},
        )
        with urlopen(req, timeout=10):
            pass

    lookup = "/api/project_branches/list?" + urlencode({"project": key})
    try:
        request(lookup)
        return
    except HTTPError as error:
        if error.code != 404:
            raise

    try:
        request("/api/projects/create", {"project": key, "name": name, "visibility": "private"})
    except HTTPError as error:
        if error.code not in (400, 409):
            raise
        # Another runner may have created the project after the lookup.
        request(lookup)


def sonar_request(url, token, path, *, query=None, data=None):
    endpoint = url + path + ("?" + urlencode(query) if query else "")
    request = Request(endpoint, data=urlencode(data).encode() if data else None,
                      headers={"Authorization": f"Bearer {token}"})
    with urlopen(request, timeout=10) as response:
        body = response.read()
        return json.loads(body) if body else {}


def push_baseline(job, checkout, env, sonar_url, token, project, branch, default_branch):
    before, pushed_at, repository_id = job.get("baseRevision"), job.get("pushedAt"), job.get("repositoryId")
    revision = job["revision"]
    if not before or not pushed_at or not repository_id or before == revision or not re.fullmatch(r"(?:[a-f0-9]{40}|[a-f0-9]{64})", before):
        return []
    try:
        run(["git", "merge-base", "--is-ancestor", before, revision], checkout, env)
    except subprocess.CalledProcessError:
        return []
    query = {"project": project}
    if branch != default_branch:
        query["branch"] = branch
    authors = run(["git", "log", "--format=%an%x09%ae", f"{before}..{revision}"],
                  checkout, env, capture=True).splitlines()
    authors = set(authors)
    author = next(iter(authors)).split("\t", 1)[0] if len(authors) == 1 else ""
    properties = [f"-Dsonar.analysis.pushBase={before}", f"-Dsonar.analysis.pushTime={pushed_at}",
                  f"-Dsonar.analysis.pushAuthor={author}"]
    verified = False
    try:
        analyses = sonar_request(sonar_url, token, "/api/project_analyses/search", query={**query, "ps": 100})["analyses"]
        baseline = next((row["key"] for row in analyses if row.get("revision") == before), None)
        if baseline:
            manager_file = os.environ.get("SONAR_BASELINE_TOKEN_FILE")
            manager = Path(manager_file).read_text().strip() if manager_file else token
            sonar_request(sonar_url, manager, "/api/new_code_periods/set",
                          data={**query, "type": "SPECIFIC_ANALYSIS", "value": baseline})
            verified = True
    except (OSError, KeyError, ValueError) as error:
        print(f"Exact Sonar push baseline unavailable: {error}", file=sys.stderr)
    return properties + [f"-Dsonar.analysis.pushBaselineVerified={str(verified).lower()}"]


def configured_baseline(repository_id, revision, branch):
    try:
        return queue_request("baseline", {"repositoryId": repository_id, "revision": revision, "branch": branch or ""})
    except (OSError, KeyError, ValueError) as error:
        print(f"Could not read Codeup Push baseline: {error}", file=sys.stderr)
        return {}


def wait_for_analysis(url, token, task):
    until = time.monotonic() + 360
    while time.monotonic() < until:
        result = sonar_request(url, token, "/api/ce/task", query={"id": task})["task"]["status"]
        if result not in ("PENDING", "IN_PROGRESS"):
            return result == "SUCCESS"
        time.sleep(3)
    raise TimeoutError("SonarQube push analysis did not finish")


def run(args, cwd, env, capture=False, timeout=45):
    return subprocess.run(
        args, cwd=cwd, env=env, check=True,
        stdout=subprocess.PIPE if capture else sys.stderr, text=True,
        timeout=timeout,
    ).stdout


def fetch_checkout(repository, branch, checkout, env):
    ref = f"refs/heads/{branch}"
    timeout = int(env.get("CODEUP_GIT_FETCH_TIMEOUT", "180"))
    cache_root = env.get("CODEUP_GIT_CACHE_DIR")
    if not cache_root:
        run(["git", "init", "--quiet", str(checkout)], checkout.parent, env)
        run(["git", "remote", "add", "origin", repository], checkout, env)
        run(["git", "fetch", "--quiet", "--no-tags", "origin", ref], checkout, env, timeout=timeout)
        return "FETCH_HEAD"

    root = Path(cache_root).expanduser().resolve()
    root.mkdir(parents=True, exist_ok=True, mode=0o700)
    identity = hashlib.sha256(repository.encode()).hexdigest()
    cache = root / f"{identity}.git"
    with (root / f"{identity}.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        if not (cache / "HEAD").is_file():
            run(["git", "init", "--quiet", "--bare", str(cache)], root, env)
        run(["git", "fetch", "--quiet", "--no-tags", repository, f"+{ref}:{ref}"],
            cache, env, timeout=timeout)
        clone_mode = "--local" if cache.stat().st_dev == checkout.parent.stat().st_dev else "--no-hardlinks"
        run(["git", "clone", "--quiet", clone_mode, "--no-checkout", "--single-branch",
             "--branch", branch, "--", str(cache), str(checkout)], root, env)
    run(["git", "remote", "set-url", "origin", repository], checkout, env)
    return f"refs/remotes/origin/{branch}"


def scan(job, repository, default_branch, base_key, *args, **kwargs):
    branch = job["branch"] or default_branch
    directory = Path.home() / ".cache/shepherd-sonar"
    directory.mkdir(parents=True, exist_ok=True, mode=0o700)
    key = hashlib.sha256(f"{base_key}:{branch}".encode()).hexdigest()
    with (directory / key).open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        return scan_checkout(job, repository, default_branch, base_key, *args, **kwargs)


def scan_checkout(job, repository, default_branch, base_key, mode, sonar_url, credential, exclusions="", *, use_repository_settings=True, branch_analysis=False,
                  project_name=None, rename_only=False, isolated_branch=False):
    branch = job["branch"] or default_branch
    revision = job["revision"]
    if not re.fullmatch(r"(?:[a-f0-9]{40}|[a-f0-9]{64})", revision):
        raise ValueError("Invalid queue revision")
    ref = f"refs/heads/{branch}"
    key = base_key
    result = {"projectKey": key, "branch": branch, "revision": revision}
    if branch != default_branch and not branch_analysis and not isolated_branch:
        return dict(result, status="branch_not_supported")
    env = {k: v for k, v in os.environ.items() if k not in (
        "SONAR_TOKEN", "CODEUP_QUALITY_QUEUE_TOKEN", "CODEUP_SONAR_TOKEN_FILE",
        "CODEUP_WEBHOOK_TOKEN",
    )}
    with TemporaryDirectory(prefix="codeup-sonar-") as scratch:
        checkout = Path(scratch) / "checkout"
        run(["git", "check-ref-format", ref], scratch, env)
        started = time.monotonic()
        print(f"Fetching {key} / {branch}", file=sys.stderr, flush=True)
        try:
            fetched_ref = fetch_checkout(repository, branch, checkout, env)
        except subprocess.CalledProcessError:
            probe = subprocess.run(
                ["git", "ls-remote", "--exit-code", repository, ref],
                cwd=scratch, env=env, stdout=subprocess.DEVNULL, timeout=45,
            )
            if probe.returncode == 2:
                return dict(result, status="superseded")
            raise
        try:
            run(["git", "merge-base", "--is-ancestor", revision, fetched_ref], checkout, env)
        except subprocess.CalledProcessError:
            return dict(result, status="superseded")
        run(["git", "checkout", "--quiet", "-B", branch, revision], checkout, env)
        print(f"Checkout ready in {time.monotonic() - started:.1f}s", file=sys.stderr, flush=True)
        if mode == "maven":
            try:
                run(["mvn", "-B", "-ntp", "clean", "install"], checkout, env, timeout=720)
            except (subprocess.CalledProcessError, subprocess.TimeoutExpired):
                return dict(result, status="build_failed")

        token = credential.read_text().strip()
        if not token:
            raise ValueError("Empty SonarQube token file")
        try:
            ensure_sonar_project(sonar_url, token, key, project_name or key)
        except OSError as error:
            print(f"SonarQube project provisioning failed: {error}", file=sys.stderr)
            return dict(result, status="scan_failed")

        author = run(["git", "show", "-s", "--format=%an", revision],
                     checkout, env, capture=True).strip()
        metadata = Path(scratch) / "report-task.txt"
        scanner = ["sonar-scanner"] if mode == "cli" else [
            "mvn", "-B", "-ntp", "org.sonarsource.scanner.maven:sonar-maven-plugin:5.5.0.6356:sonar",
        ]
        if not use_repository_settings:
            settings = Path(scratch) / "sonar-project.properties"
            settings.write_text("sonar.sources=.\n")
            scanner.append(f"-Dproject.settings={settings}")
        if branch != default_branch and not isolated_branch:
            scanner.append(f"-Dsonar.branch.name={branch}")
        if project_name:
            scanner.append(f"-Dsonar.projectName={project_name}")
        if isolated_branch:
            scanner.append("-Dsonar.analysis.bulkImport=true")
        if rename_only:
            scanner.append("-Dsonar.analysis.repositoryRename=true")
        codeup_path = re.fullmatch(r"git@codeup\.aliyun\.com:([A-Za-z0-9_./-]+)\.git", repository)
        if codeup_path:
            scanner.append(f"-Dsonar.analysis.repositoryUrl=https://codeup.aliyun.com/{codeup_path.group(1)}")
        if job.get("repositoryId"):
            scanner.append(f"-Dsonar.analysis.repositoryId={job['repositoryId']}")
        if exclusions:
            scanner.append(f"-Dsonar.exclusions={exclusions}")
        baseline = []
        if not rename_only:
            try:
                baseline = push_baseline(job, checkout, env, sonar_url, token, key, branch, branch if isolated_branch else default_branch)
            except (OSError, KeyError, ValueError) as error:
                print(f"Exact Sonar push baseline unavailable: {error}", file=sys.stderr)
        try:
            run(scanner + baseline + [
                f"-Dsonar.host.url={sonar_url}", f"-Dsonar.projectKey={key}",
                f"-Dsonar.scm.revision={revision}", f"-Dsonar.analysis.gitBranch={branch}",
                f"-Dsonar.analysis.commitAuthor={author}", f"-Dsonar.scanner.metadataFilePath={metadata}",
            ], checkout, dict(env, SONAR_TOKEN=token), timeout=720)
            task = dict(line.split("=", 1) for line in metadata.read_text().splitlines() if "=" in line)
            if not task.get("ceTaskId"):
                raise ValueError("Missing SonarQube task ID")
            if "-Dsonar.analysis.pushBaselineVerified=true" in baseline and not wait_for_analysis(sonar_url, token, task["ceTaskId"]):
                return dict(result, status="scan_failed")
        except (OSError, ValueError, subprocess.SubprocessError) as error:
            print(error, file=sys.stderr)
            return dict(result, status="scan_failed")
        return dict(result, status="submitted", taskId=task["ceTaskId"])


def discovered_repository(repository_id):
    organization = os.environ["CODEUP_SONAR_ORGANIZATION"]
    token = Path(os.environ["CODEUP_SONAR_CODEUP_TOKEN_FILE"]).read_text().strip()
    endpoint = "https://openapi-rdc.aliyuncs.com/oapi/v1/codeup/organizations"
    request = Request(
        f"{endpoint}/{quote(organization, safe='')}/repositories/{repository_id}",
        headers={"x-yunxiao-token": token, "Accept": "application/json"},
    )
    try:
        with urlopen(request, timeout=15) as response:
            metadata = json.load(response)
    except HTTPError as error:
        if error.code in (403, 404):
            return None
        raise
    path = metadata.get("pathWithNamespace", "")
    branch = metadata.get("defaultBranch")
    if (
        str(metadata.get("id")) != repository_id
        or not path.startswith(organization + "/")
        or not re.fullmatch(r"[A-Za-z0-9_-][A-Za-z0-9_.-]*(?:/[A-Za-z0-9_-][A-Za-z0-9_.-]*)+", path)
        or not branch
    ):
        return None
    return f"git@codeup.aliyun.com:{path}.git", branch


def scan_discovered(repository_id, revision, branch, *, rename_only=False, push=None):
    repository = discovered_repository(repository_id)
    if repository is None:
        return {"status": "repository_unavailable", "revision": revision, "branch": branch}
    url, default_branch = repository
    branch = branch or default_branch
    native_branches = os.environ.get("SONAR_BRANCH_ANALYSIS") == "1"
    isolated_branch = branch != default_branch and not native_branches
    project_key = f"codeup:{repository_id}"
    project_name = url.rsplit("/", 1)[-1].removesuffix(".git")
    if isolated_branch:
        project_key += ":branch:" + hashlib.sha256(branch.encode()).hexdigest()[:16]
        project_name += f" · {branch}"
    return scan(
        {"revision": revision, "branch": branch, "repositoryId": repository_id, **(push or {})}, url, default_branch,
        project_key, "cli", os.environ["SONAR_HOST_URL"].rstrip("/"),
        Path(os.environ["CODEUP_SONAR_TOKEN_FILE"]).resolve(strict=True),
        os.environ.get("CODEUP_SONAR_EXCLUSIONS", ""),
        use_repository_settings=False,
        project_name=project_name,
        rename_only=rename_only,
        branch_analysis=native_branches,
        isolated_branch=isolated_branch,
    )


def poll_discovery():
    job = queue_request("claim", {"kind": "sonar_discovered"})["job"]
    if job is None:
        return
    try:
        push = {key: job[key] for key in ("baseRevision", "pushedAt") if key in job}
        result = scan_discovered(job["repositoryId"], job["revision"], job["branch"], push=push)
    except (OSError, ValueError, subprocess.SubprocessError):
        queue_request("finish", {"lease": job["lease"], "successful": False})
        raise
    queue_request("finish", {"lease": job["lease"], "successful": True, "result": result})
    print(json.dumps(dict(result, repositoryId=job["repositoryId"])))


def main():
    config = os.environ
    command = os.environ.get("SSH_ORIGINAL_COMMAND")
    if len(sys.argv) == 6 and sys.argv[1] == "--scan-metadata":
        _, _, repository_id, revision, branch, project = sys.argv
        print(f"-Dsonar.analysis.repositoryId={repository_id}")
        job = {"revision": revision, "repositoryId": repository_id,
               **configured_baseline(repository_id, revision, "" if branch == "master" else branch)}
        for prop in push_baseline(job, Path.cwd(), config, config["SONAR_HOST_URL"],
                                  config["SONAR_TOKEN"], project, branch, "master"):
            print(prop)
        return
    if command is None and sys.argv[1:] == ["--discover-once"]:
        poll_discovery()
        return
    match = None
    if command is not None:
        match = re.fullmatch(
            r"scan-codeup ([0-9]+) ([a-f0-9]{40}|[a-f0-9]{64})(?: ([A-Za-z0-9_][A-Za-z0-9_./-]{0,159}))?",
            command.removeprefix("cd / ; "),
        )
        if not match:
            raise SystemExit("Expected scan-codeup followed by a repository ID, SHA, and optional branch")
        repository_id = match[1]
        registry_path = os.environ.get("CODEUP_SONAR_REPOSITORY_CONFIG")
        if registry_path:
            registry = json.loads(Path(registry_path).read_text())
            if repository_id not in registry:
                raise SystemExit("Repository has no local scanner configuration")
            config = {**os.environ, **registry[repository_id]}
        elif repository_id != os.environ["CODEUP_SONAR_REPOSITORY_ID"]:
            raise SystemExit("Expected scan-codeup for the configured repository")
    else:
        repository_id = config["CODEUP_SONAR_REPOSITORY_ID"]

    repository = config["CODEUP_SONAR_REPOSITORY_URL"]
    default_branch = config["CODEUP_DEFAULT_BRANCH"]
    base_key = config["SONAR_PROJECT_KEY"]
    sonar_url = config["SONAR_HOST_URL"].rstrip("/")
    credential = Path(config["CODEUP_SONAR_TOKEN_FILE"]).resolve(strict=True)
    mode = config["SONAR_SCANNER_MODE"]
    exclusions = config.get("CODEUP_SONAR_EXCLUSIONS", "")
    if mode not in ("maven", "cli"):
        raise ValueError("Set SONAR_SCANNER_MODE to maven or cli")

    branch_analysis = config.get("SONAR_BRANCH_ANALYSIS") == "1"
    if config.get("SONAR_BASELINE_TOKEN_FILE"):
        os.environ["SONAR_BASELINE_TOKEN_FILE"] = config["SONAR_BASELINE_TOKEN_FILE"]
    if match:
        job = {"revision": match[2], "branch": match[3], "repositoryId": repository_id,
               **configured_baseline(repository_id, match[2], match[3])}
        print(json.dumps(scan(job, repository, default_branch, base_key, mode, sonar_url, credential, exclusions,
                              branch_analysis=branch_analysis)))
        return

    job = queue_request("claim", {"kind": "sonar", "repositoryId": repository_id})["job"]
    if job is None:
        return
    try:
        result = scan(job, repository, default_branch, base_key, mode, sonar_url, credential,
                      exclusions, branch_analysis=branch_analysis)
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        print(error, file=sys.stderr)
        queue_request("finish", {"lease": job["lease"], "successful": False})
        raise SystemExit(1) from error
    queue_request("finish", {"lease": job["lease"], "successful": True, "result": result})
    print(json.dumps(result))


if __name__ == "__main__":
    main()
