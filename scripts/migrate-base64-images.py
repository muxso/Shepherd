#!/usr/bin/env python3
"""Migrate legacy inline base64 images in requirement descriptions to project files.

Before server-side upload existed, the markdown editor embedded images as
`![alt](data:image/...;base64,...)`. This script finds those blobs in the
latest version of every requirement description, uploads each image via
POST /api/project-file, rewrites the markdown to `/api/project-file/{id}/raw`,
and publishes the rewritten description as a new requirement version
(POST /requirement/{id}/version). If the baseline pointed at the previous
latest version, the baseline is moved to the new version.

Auth and transport go through the HTTP API only (no direct DB access).

Usage:
  python3 scripts/migrate-base64-images.py            # dry-run (default)
  python3 scripts/migrate-base64-images.py --apply    # perform the migration

Environment:
  SHEPHERD_BASE            API base URL (default http://127.0.0.1:9180)
  SHEPHERD_ADMIN_USER      login user (default admin)
  SHEPHERD_ADMIN_PASSWORD  login password (default admin)

Idempotent: requirements whose description has no data:image URI are skipped.
"""

import argparse
import base64
import json
import os
import re
import sys
import urllib.error
import urllib.request

BASE = os.environ.get("SHEPHERD_BASE", "http://127.0.0.1:9180").rstrip("/")
USER = os.environ.get("SHEPHERD_ADMIN_USER", "admin")
PASSWORD = os.environ.get("SHEPHERD_ADMIN_PASSWORD", "admin")

# Markdown image with a data:image URI. Base64 alphabet has no ')', so a
# greedy-free char class terminates correctly at the closing paren.
DATA_IMG_RE = re.compile(
    r"!\[([^\]\n]*)\]\(\s*data:image/([a-zA-Z0-9.+-]+);base64,([A-Za-z0-9+/=\s]+?)\s*\)"
)

EXT_BY_SUBTYPE = {"jpeg": "jpg", "svg+xml": "svg"}


def api(method: str, path: str, body=None, token: str | None = None):
    url = BASE + path
    data = json.dumps(body).encode() if body is not None else None
    req = urllib.request.Request(url, data=data, method=method)
    if data is not None:
        req.add_header("Content-Type", "application/json")
    if token:
        req.add_header("Authorization", f"Bearer {token}")
    try:
        with urllib.request.urlopen(req, timeout=60) as res:
            raw = res.read()
    except urllib.error.HTTPError as e:
        detail = e.read().decode(errors="replace")[:300]
        raise RuntimeError(f"{method} {path} -> {e.code}: {detail}") from e
    return json.loads(raw) if raw else None


def fmt_size(n: int) -> str:
    if n >= 1024 * 1024:
        return f"{n / 1024 / 1024:.1f}MB"
    if n >= 1024:
        return f"{n / 1024:.1f}KB"
    return f"{n}B"


def migrate_description(desc: str, project_id: str, req_label: str, token: str, apply: bool):
    """Return (new_desc, images) where images is a list of (name, size_bytes)."""
    images = []
    counter = [0]

    def repl(m: re.Match) -> str:
        counter[0] += 1
        alt, subtype, b64 = m.group(1), m.group(2), re.sub(r"\s+", "", m.group(3))
        try:
            blob = base64.b64decode(b64, validate=False)
        except Exception:
            print(f"    [warn] image {counter[0]}: undecodable base64, left as-is")
            return m.group(0)
        ext = EXT_BY_SUBTYPE.get(subtype, subtype)
        name = f"{req_label}-img{counter[0]}.{ext}"
        images.append((name, len(blob)))
        if not apply:
            return m.group(0)
        r = api("POST", "/api/project-file", {
            "projectId": project_id,
            "name": name,
            "fileFormat": ext,
            "sizeBytes": len(blob),
            "contentBase64": b64,
            "moduleId": "markdown",
        }, token)
        return f"![{alt}](/api/project-file/{r['id']}/raw)"

    return DATA_IMG_RE.sub(repl, desc), images


def main() -> int:
    ap = argparse.ArgumentParser(description="Migrate inline base64 images to project files")
    ap.add_argument("--apply", action="store_true", help="perform the migration (default: dry-run)")
    args = ap.parse_args()
    mode = "APPLY" if args.apply else "DRY-RUN"

    token = api("POST", "/auth/login", {"username": USER, "password": PASSWORD})["token"]

    orgs = api("GET", "/organization?pageSize=100", token=token)
    projects = []
    for org in orgs.get("items") or []:
        page = api("GET", f"/project?organizationId={org['id']}&pageSize=100", token=token)
        projects += page.get("items") or []
    print(f"[{mode}] base={BASE} orgs={len(orgs.get('items') or [])} projects={len(projects)}")

    scanned = hits = migrated = 0
    total_bytes = 0
    for p in projects:
        page = api("GET", f"/requirement?projectId={p['id']}&current=1&pageSize=500", token=token)
        reqs = page.get("items") or []
        for item in reqs:
            scanned += 1
            r = api("GET", f"/requirement/{item['id']}", token=token)
            latest_n = r["latestVersion"]
            latest = next(v for v in r["versions"] if v["version"] == latest_n)
            desc = latest["description"]
            if "data:image" not in desc:
                # Report legacy blobs stuck in immutable older versions.
                for v in r["versions"]:
                    if v["version"] != latest_n and "data:image" in v["description"]:
                        print(f"  [note] {p['name']} / {r['title']}: base64 only in old v{v['version']} (immutable, skipped)")
                continue
            hits += 1
            label = f"req-{r.get('num') or r['id'][:8]}"
            new_desc, images = migrate_description(desc, p["id"], label, token, args.apply)
            size = sum(s for _, s in images)
            total_bytes += size
            print(f"  {p['name']} / {r['title']} (v{latest_n}): {len(images)} image(s), {fmt_size(size)}")
            for name, s in images:
                print(f"    - {name} ({fmt_size(s)})")
            if not args.apply:
                continue
            if new_desc == desc:
                print("    [warn] nothing rewritten (undecodable images only), skipped")
                continue
            created = api("POST", f"/requirement/{r['id']}/version", {
                "description": new_desc,
                "acceptanceCriteria": latest["acceptanceCriteria"],
            }, token)
            new_n = created["version"]
            if r["baselineVersion"] == latest_n:
                api("PUT", f"/requirement/{r['id']}/baseline", {"version": new_n}, token)
            migrated += 1
            print(f"    -> migrated as v{new_n}" + (" (baseline moved)" if r["baselineVersion"] == latest_n else ""))

    print(f"[{mode}] scanned={scanned} with-base64={hits} migrated={migrated} payload={fmt_size(total_bytes)}")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except RuntimeError as e:
        print(f"error: {e}", file=sys.stderr)
        sys.exit(1)
