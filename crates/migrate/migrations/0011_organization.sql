-- 组织表。部分唯一索引:未删除组织内名称唯一(与 project 同手法)。
CREATE TABLE ms_organization (
    seq     BIGSERIAL,
    id      TEXT PRIMARY KEY DEFAULT gen_random_uuid()::text,
    name    TEXT NOT NULL,
    enable  BOOLEAN NOT NULL DEFAULT true,
    deleted BOOLEAN NOT NULL DEFAULT false
);
CREATE UNIQUE INDEX ux_ms_org_name_active ON ms_organization (name) WHERE deleted = false;
