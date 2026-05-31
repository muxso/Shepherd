-- delivery (Shepherd 交付执行):任务派发给 AI 执行者的交付尝试记录。
CREATE TABLE ms_delivery_attempt (
    seq                    BIGSERIAL,
    id                     TEXT PRIMARY KEY DEFAULT gen_random_uuid()::text,
    decomposition_id       TEXT NOT NULL,
    task_id                TEXT NOT NULL,
    executor               TEXT NOT NULL,                 -- CLAUDE_CODE / CODEX
    status                 TEXT NOT NULL DEFAULT 'DISPATCHED',
    run_id                 TEXT,                          -- 异步执行者回调关联用
    deliverable_kind       TEXT,                          -- DIFF / PULL_REQUEST / BRANCH / PATCH
    deliverable_reference  TEXT,
    deliverable_summary    TEXT,
    error                  TEXT
);
CREATE INDEX ix_ms_delivery_attempt_task ON ms_delivery_attempt (decomposition_id, task_id);
