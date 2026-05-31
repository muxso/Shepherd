-- 鉴权:用户凭证 + 会话。permissions 用 Postgres text[](免 JSON 依赖,sqlx 原生映射 Vec<String>)。
CREATE TABLE ms_user_credential (
    username      TEXT PRIMARY KEY,
    user_id       TEXT NOT NULL,
    password_hash TEXT NOT NULL,            -- Argon2 PHC 串
    permissions   TEXT[] NOT NULL DEFAULT '{}'   -- 原始权限串,如 'SYSTEM_USER:READ+ADD'
);

CREATE TABLE ms_session (
    token       TEXT PRIMARY KEY DEFAULT gen_random_uuid()::text,
    user_id     TEXT NOT NULL,
    permissions TEXT[] NOT NULL DEFAULT '{}',
    created_at  BIGINT,
    expires_at  BIGINT NOT NULL DEFAULT 0
);
