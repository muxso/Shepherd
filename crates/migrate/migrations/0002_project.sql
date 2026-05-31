-- project:项目表。部分唯一索引固化"同组织内未删除项目名唯一"。
CREATE TABLE ms_project (
    seq             BIGSERIAL,
    id              TEXT PRIMARY KEY DEFAULT gen_random_uuid()::text,
    organization_id TEXT NOT NULL,
    name            TEXT NOT NULL,
    enable          BOOLEAN NOT NULL DEFAULT true,
    deleted         BOOLEAN NOT NULL DEFAULT false
);

CREATE UNIQUE INDEX ux_ms_project_org_name_active
    ON ms_project (organization_id, name) WHERE deleted = false;
