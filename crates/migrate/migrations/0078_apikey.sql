-- 每 agent 独立 API Key(sak_<id>.<secret>),替代 agent-runtime 共享管理员口令登录。
-- secret 只存 Argon2 哈希;吊销走 revoked 标记,保留审计记录。
CREATE TABLE IF NOT EXISTS ms_apikey (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL UNIQUE,
    secret_hash TEXT NOT NULL,
    permissions TEXT[] NOT NULL DEFAULT '{}',
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    revoked     BOOLEAN NOT NULL DEFAULT false
);
