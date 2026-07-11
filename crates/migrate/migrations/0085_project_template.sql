-- 项目内模板:kind 区分场景(requirement / functional-case / bug ...),同表扩展。
-- config 对后端不透明(JSONB),结构由各前端场景自定义。
CREATE TABLE IF NOT EXISTS ms_template (
    id         TEXT PRIMARY KEY DEFAULT gen_random_uuid()::text,
    project_id TEXT NOT NULL,
    kind       TEXT NOT NULL,
    name       TEXT NOT NULL,
    config     JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_by TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (project_id, kind, name)
);
CREATE INDEX IF NOT EXISTS ix_template_proj_kind ON ms_template (project_id, kind);
