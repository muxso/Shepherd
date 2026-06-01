-- skill (Shepherd AI Skill 编排):可复用、可组合的 AI 行为规范。
CREATE TABLE ms_skill (
    seq           BIGSERIAL,
    id            TEXT PRIMARY KEY DEFAULT gen_random_uuid()::text,
    project_id    TEXT NOT NULL,
    name          TEXT NOT NULL,
    description   TEXT NOT NULL DEFAULT '',
    instructions  TEXT NOT NULL,
    includes      TEXT[] NOT NULL DEFAULT '{}',   -- 组合:包含的其它 skill id
    enabled       BOOLEAN NOT NULL DEFAULT true,
    deleted       BOOLEAN NOT NULL DEFAULT false
);
CREATE UNIQUE INDEX ux_ms_skill_proj_name_active
    ON ms_skill (project_id, name) WHERE deleted = false;
