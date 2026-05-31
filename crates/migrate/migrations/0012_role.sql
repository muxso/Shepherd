-- 角色 + 用户-角色授权。permissions 用 text[]。
CREATE TABLE ms_role (
    seq         BIGSERIAL,
    id          TEXT PRIMARY KEY DEFAULT gen_random_uuid()::text,
    name        TEXT NOT NULL,
    scope       TEXT NOT NULL,
    permissions TEXT[] NOT NULL DEFAULT '{}'
);
CREATE TABLE ms_user_role (
    user_id TEXT NOT NULL,
    role_id TEXT NOT NULL,
    PRIMARY KEY (user_id, role_id)
);
CREATE INDEX ix_user_role_user ON ms_user_role (user_id);
