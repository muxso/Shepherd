# Code repository RAG indexer

Optional read-only Yunxiao deployment worker. It sends code through the existing authenticated
Shepherd RAG API; credentials, clones and checkpoints stay outside the application checkout.
The repository manifest is an explicit allowlist, not a claim that every project member has
access to every Codeup repository. Default visibility is uploader/admin only. Use separate
project/config/state directories for different repository audiences; set `visibilityGroups` only
to existing role groups approved for every repository in that manifest. Project membership and
RAG permissions remain required by the host API.

Example private configuration (chmod 600; replace all placeholders):

```json
{
  "apiUrl": "http://127.0.0.1:8088",
  "projectId": "PROJECT_UUID",
  "tokenFile": "/home/service/.config/shepherd/token",
  "credentialsFile": "/home/service/.config/shepherd/credentials.json",
  "repositoriesFile": "/home/service/.config/shepherd/repositories.json",
  "stateDir": "/home/service/.local/state/shepherd-code-rag",
  "embeddingRevision": "bge-m3:567m@MODEL_DIGEST/dim1024",
  "visibilityGroups": [],
  "workers": 2
}
```

Manifest format:

```json
[
  {"id": "REPOSITORY_ID", "name": "team/repository", "url": "git@codeup.aliyun.com:org/team/repository.git", "branch": "main"}
]
```

Use an existing read-only SSH identity with preapproved host keys. Do not put credentials in clone
URLs. `python3 index.py --config /absolute/private/config.json` clones bare shallow default branches,
reads regular committed blobs without checkout/execution, and sends one stable document per file.
Code chunks retain repository, branch, full commit and exact line range. Each code line carries an explicit `L<number>:` label so generated method references can use actual line numbers. Format revision 1 checkpoints automatically migrate to revision 2 by re-embedding every tracked file while retaining stable document IDs and deletion tracking; all other identity changes still require explicit migration. File allowlist, size cap,
UTF-8 validation, credential-line redaction and private-key rejection run before upload. This is
conservative filtering, not a comprehensive secret scanner: review source access before sharing.

Checkpoints are written atomically after each embedded file. Blob IDs skip unchanged content.
Deleted/renamed files are removed only after the full repository ingest succeeds. Failures return
nonzero and are retried on the next run; SIGTERM cancellation preserves completed checkpoints.
Per-repository locks prevent concurrent modification. A new embedding model, project, audience or
branch requires a fresh state directory and explicit retirement of old documents. Unchanged file
citations keep their original content commit; the repository checkpoint records latest scanned SHA.
Removed repositories are not implicitly deleted: revoke their document audience or explicitly purge
their indexed documents. Empty repositories should be recorded in inventory and omitted from manifest.

For periodic scans use the supplied systemd units, install the scripts in
`~/shepherd-code-rag`, and supply a fresh scoped session in `tokenFile` before each run. Alternatively provide a private
`credentialsFile` containing `username` and `password` for a dedicated account with only
`RAG:READ/ADD/DELETE` and membership in the target project; the worker logs in at start and every
30 minutes. Never use an administrator password for the worker. If a session
expires mid-scan, refresh it and rerun; successful files are retained. Do not log tokens or upstream
response bodies. `summary.json` and per-repository checkpoints record completion; `indexing` after a
stopped process means interrupted, not complete. Newly created repositories require manifest refresh in manual mode; the optional Codeup binding below refreshes automatically before every run.

Ollama configuration in System parameters → RAG:

- Embedding URL: `http://shepherd-ollama:11434/v1/embeddings`, model `bge-m3:567m`, dimension 1024,
  local compatibility key `ollama`. Confirm dimension against the actual installed model.
- Chat URL: `http://shepherd-ollama:11434/api/chat`, model `qwen3:8b`. Native endpoint disables thinking
  and streams NDJSON answer content; OpenAI-compatible providers retain their existing SSE payload.
- Connect Ollama and Shepherd to a persistent internal Docker network with alias `shepherd-ollama`.
  Keep host Ollama port bound to loopback. Preserve existing GPU runtime and model volume.
- Probe both providers through `/system/rag/test`, then ask a code question and inspect cited file,
  line range and commit. Large first scans run asynchronously; a successful chat is not proof the
  entire manifest is indexed.

Tests: `python3 -m unittest discover -s plugins/yunxiao/scripts/code_rag -p 'test_*.py'`.

## Large libraries: optional Qdrant ANN index

Set server environment `SHEPHERD_RAG_QDRANT_COLLECTION_URL` to an internal URL such as
`http://shepherd-qdrant:6333/collections/shepherd_rag_bge_m3_1024_v1` (optional
`SHEPHERD_RAG_QDRANT_KEY`). Provision a **new empty** collection with Cosine distance and exactly
the configured embedding dimension, plus keyword payload indexes on `project_id` and `document_id`.
The collection is dedicated to this Shepherd installation and embedding revision. Do not attach an
empty side index to an existing populated knowledge base without fully re-ingesting its documents.
There is no automatic migration of already embedded rows. Changing models requires a new collection
and complete re-ingest; keep old collection/PG backup until verification finishes.

The side index contains vectors and project/document IDs, never source text. Ingest/reindex updates
it, document deletion cleans it, and Qdrant failures are reported rather than silently returning
incomplete results. PostgreSQL remains authoritative: every ANN candidate is rehydrated with the
current project and audience checks, so revoked/deleted documents cannot leak via a stale index.
ANN over-fetches up to 1000 candidates before authorization; highly restricted users may get fewer
hits. Without this environment variable the existing PostgreSQL cosine path remains available.


## Cover the Yunxiao /devops repository scope automatically

To index every repository visible to the organization bound to a Devops source, add an optional
`codeup` block to the private worker configuration:

```json
{
  "codeup": {
    "sourceId": "YUNXIAO_SOURCE_UUID",
    "organizationId": "CODEUP_ORGANIZATION_ID",
    "requiredRepositoryIds": ["REPOSITORY_ID_FROM_SOURCE_HISTORY"],
    "envFile": "/home/service/.config/devops-weekly/env",
    "devopsSnapshotFile": "/home/service/.cache/shepherd-yunxiao/snapshot.json"
  }
}
```

Keep `codeup_inventory.py` next to `index.py`. Bind `projectId` to that Devops source's Shepherd
project and validate the source organization at provisioning. The environment file supplies existing
`YUNXIAO_ORG`/`YUNXIAO_TOKEN` read credentials; the worker's Shepherd account still needs only RAG
permissions and membership in that project. No credentials are written to manifests or logs.

Before every scan the worker reads **all pages** of Codeup's organization-visible repositories and
fetches each default branch. It atomically updates the manifest, retaining existing priority and adding
new repositories. Empty repositories are recorded separately. A local Devops snapshot, when configured,
audits that every repository referenced by its commit records appears in the inventory. Seed `requiredRepositoryIds` from the source’s full retained commit history at provisioning; observed repository IDs are retained across later snapshots so rolling collection windows do not drop coverage obligations. Missing access,
incomplete detail or upstream failure aborts refresh and retains the previous manifest; the run fails
instead of claiming complete coverage. `inventory.json` beside the manifest records the bound source,
repository counts, empty repositories, coverage audit, timestamp and manifest checksum. Git checkpoints
still provide per-file progress and commit versions. Repository removal and branch changes retain the
explicit retirement/migration rules above; visibility is never broadened automatically.


Embedding context limits are token-based, not character-based. The host retries ingestion with
progressively smaller source chunks only when the provider explicitly reports an input-context
limit. All original source lines and their references are retained; data is never silently truncated
or converted to keyword-only after a configured provider fails. Retry stops at the minimum chunk size
and reports an error if it still cannot fit. Completed files keep their existing checkpoints and do
not need re-embedding when this retry behavior is deployed.

## Daily source, Git activity and Yunxiao knowledge refresh

The code timer runs every day at **02:00 Asia/Shanghai** (plus up to five minutes of jitter),
with boot catch-up and persistent scheduling. Set `gitHistoryDays` (1–366, deployed value 90)
to fetch all remote branches within a rolling history window. Latest code files still follow the
configured default branch. Remote tips are checked before deepening: dormant repositories skip
history deepening, and unchanged branches reuse the verified local history window. Commit evidence excludes merges and deduplicates repository + SHA;
author dates are converted to Shanghai days, and committer timestamps remain in the evidence.
`authorAliasesFile` optionally names a private JSON map from Git names/emails to approved canonical
names. The worker publishes stable per-author/day documents only after all repositories succeed;
older already-indexed daily facts remain archived outside the current fetch window. Git activity
is evidence of commits, not a claim about hours, complete work, or performance.

Questions such as “王震昨天干了什么”, “cxl 在 2026-11-04 的提交记录” and “团队本周工作总结”
use exact author/alias + calendar-date retrieval before generation. Project and current document
audiences are checked for metadata selection and again when hydrating evidence. No matching
person/day produces no evidence rather than borrowing another person's semantic matches. Up to
31 author/day documents and a bounded context are returned; the context states the requested
range, matched count and selection limit. Retrieved excerpts retain commit messages, repositories, SHA values and eligible changed file paths within the stated context budget; one answer may not include the entire stored document. No repository scripts or hooks run.

Install `yunxiao_documents.py` next to `index.py`, plus `shepherd-yunxiao-rag.service` and its timer.
This second job runs at **03:30 Asia/Shanghai** and reads completed plugin snapshots through the
existing permission-checked records API. Example additions to a separate private config:

```json
{
  "yunxiao": {"sourceId": "YUNXIAO_SOURCE_UUID"},
  "stateDir": "/home/service/shepherd-code-rag/yunxiao-state"
}
```

Reuse the API/project/auth/embedding/audience fields from the code worker. Its source-reader role
also needs `PROJECT:READ`, `REQUIREMENT:READ` and `FUNCTIONAL_CASE:READ`. Empty visibility groups
remain uploader/admin only; indexing never grants project-wide access automatically.

Requirements include canonical source number, title, owner/status and normalized HTML/JSONML
body. Test cases include library, directory, preconditions, ordered steps and expected results.
Image-only or missing content is explicitly reported and is not inferred. Embedded images and
linked binary attachments are not OCRed or downloaded. Source identity and content digests skip
unchanged documents; stable IDs replace changed versions. Missing source items retire only after a
complete stable source read and successful ingestion. Concurrent source revision changes or any
embedding error keep previous completed knowledge and resumable checkpoints.

Run `python3 ~/shepherd-code-rag/progress.py` for code, Git activity and cloud-document progress;
`journalctl --user -u shepherd-code-rag.service -f` and
`journalctl --user -u shepherd-yunxiao-rag.service -f` show their separate jobs. Daily job updates and
initial full import can take time; a deployed endpoint does not imply all documents are indexed.
