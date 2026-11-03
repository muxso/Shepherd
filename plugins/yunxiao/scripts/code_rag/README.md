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
Code chunks retain repository, branch, full commit and exact line range. File allowlist, size cap,
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
stopped process means interrupted, not complete. Newly created repositories require manifest refresh.

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
