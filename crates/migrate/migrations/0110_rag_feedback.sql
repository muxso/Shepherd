-- Thumbs up/down feedback on RAG answers, so answer quality can be reviewed over time.
-- One row per vote; the question/answer text is denormalized so a vote stays meaningful even if the
-- conversation (client-side only) is gone.
CREATE TABLE IF NOT EXISTS ms_rag_feedback (
    id          TEXT PRIMARY KEY DEFAULT gen_random_uuid()::text,
    project_id  TEXT NOT NULL,
    session_id  TEXT,
    user_id     TEXT,
    vote        SMALLINT NOT NULL,  -- 1 = up, -1 = down
    question    TEXT NOT NULL DEFAULT '',
    answer      TEXT NOT NULL DEFAULT '',
    comment     TEXT NOT NULL DEFAULT '',
    created_at  BIGINT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_rag_feedback_project ON ms_rag_feedback (project_id, created_at);
