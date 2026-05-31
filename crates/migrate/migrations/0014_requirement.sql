-- requirement (Shepherd 需求管理):需求聚合 + 不可变版本快照(线性多版本 + baseline 指针)。
CREATE TABLE ms_requirement (
    seq               BIGSERIAL,
    id                TEXT PRIMARY KEY DEFAULT gen_random_uuid()::text,
    project_id        TEXT NOT NULL,
    title             TEXT NOT NULL,
    status            TEXT NOT NULL DEFAULT 'DRAFT',
    baseline_version  INTEGER NOT NULL DEFAULT 1,
    deleted           BOOLEAN NOT NULL DEFAULT false
);

-- 同项目内未删除需求标题唯一(软删除后标题释放,可重建同名)。
CREATE UNIQUE INDEX ux_ms_requirement_proj_title_active
    ON ms_requirement (project_id, title) WHERE deleted = false;

-- 不可变版本快照:验收标准存为 text[];(requirement_id, version) 主键保证版本唯一且只追加。
CREATE TABLE ms_requirement_version (
    requirement_id       TEXT NOT NULL REFERENCES ms_requirement(id) ON DELETE CASCADE,
    version              INTEGER NOT NULL,
    description          TEXT NOT NULL DEFAULT '',
    acceptance_criteria  TEXT[] NOT NULL DEFAULT '{}',
    created_at           BIGINT NOT NULL DEFAULT (extract(epoch from now()) * 1000)::bigint,
    PRIMARY KEY (requirement_id, version)
);
