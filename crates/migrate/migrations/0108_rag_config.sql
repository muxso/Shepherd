-- RAG runtime configuration (single row), editable from system settings. Values override the
-- SHEPHERD_RAG_* env fallbacks. Keys are stored here; the admin API never returns them.
CREATE TABLE IF NOT EXISTS ms_rag_config (
    id          INT PRIMARY KEY DEFAULT 1 CHECK (id = 1),
    embed_url   TEXT NOT NULL DEFAULT '',
    embed_model TEXT NOT NULL DEFAULT '',
    embed_dim   INT  NOT NULL DEFAULT 1536,
    embed_key   TEXT NOT NULL DEFAULT '',
    chat_url    TEXT NOT NULL DEFAULT '',
    chat_model  TEXT NOT NULL DEFAULT '',
    chat_key    TEXT NOT NULL DEFAULT '',
    max_tokens  INT  NOT NULL DEFAULT 1500,
    top_k       INT  NOT NULL DEFAULT 8,
    rerank      BOOLEAN NOT NULL DEFAULT TRUE,
    updated_at  BIGINT NOT NULL DEFAULT 0
);
