-- 需求变更日志:每次字段级变更追加一行(只增不改),按 seq 倒序即最新在前。
CREATE TABLE ms_requirement_change (
    seq             BIGSERIAL PRIMARY KEY,
    requirement_id  TEXT NOT NULL REFERENCES ms_requirement(id) ON DELETE CASCADE,
    changed_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    changed_by      TEXT NOT NULL DEFAULT '',
    field           TEXT NOT NULL,
    old_value       TEXT NOT NULL DEFAULT '',
    new_value       TEXT NOT NULL DEFAULT ''
);

CREATE INDEX ix_requirement_change_req ON ms_requirement_change (requirement_id, seq DESC);
