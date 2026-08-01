-- RAG knowledge base: documents + chunked embeddings, stored in Postgres.
-- Embeddings are real[] with an in-DB cosine-similarity function so this works on a
-- stock Postgres (no pgvector extension). The VectorStore port stays swappable, so a
-- pgvector-backed store (vector column + HNSW) can drop in later without touching callers.

CREATE TABLE IF NOT EXISTS ms_rag_document (
    id           TEXT PRIMARY KEY,
    project_id   TEXT NOT NULL,
    source_type  TEXT NOT NULL DEFAULT 'manual',  -- manual | requirement | case | prd | ...
    source_id    TEXT,                             -- id of the originating Shepherd entity, if any
    title        TEXT NOT NULL DEFAULT '',
    created_at   BIGINT NOT NULL,
    updated_at   BIGINT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_rag_document_project ON ms_rag_document (project_id);

CREATE TABLE IF NOT EXISTS ms_rag_chunk (
    id           TEXT PRIMARY KEY,
    document_id  TEXT NOT NULL REFERENCES ms_rag_document(id) ON DELETE CASCADE,
    project_id   TEXT NOT NULL,
    chunk_index  INT NOT NULL,
    heading      TEXT NOT NULL DEFAULT '',
    content      TEXT NOT NULL,
    embedding    REAL[] NOT NULL,
    created_at   BIGINT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_rag_chunk_project ON ms_rag_chunk (project_id);
CREATE INDEX IF NOT EXISTS idx_rag_chunk_document ON ms_rag_chunk (document_id);

-- Cosine similarity between two equal-length real[] vectors, computed in-DB.
CREATE OR REPLACE FUNCTION rag_cosine(a REAL[], b REAL[]) RETURNS DOUBLE PRECISION AS $$
    SELECT CASE WHEN la = 0 OR lb = 0 THEN 0
                ELSE dot / (sqrt(la) * sqrt(lb)) END
    FROM (
        SELECT sum(x::double precision * y) AS dot,
               sum(x::double precision * x) AS la,
               sum(y::double precision * y) AS lb
        FROM unnest(a, b) AS t(x, y)
    ) s
$$ LANGUAGE sql IMMUTABLE;
