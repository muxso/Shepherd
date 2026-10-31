CREATE TABLE ep_secret (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    environment_id TEXT,
    label TEXT NOT NULL,
    key_id TEXT NOT NULL,
    nonce BYTEA NOT NULL CHECK(octet_length(nonce)=12),
    ciphertext BYTEA NOT NULL CHECK(octet_length(ciphertext)>=16),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX ix_ep_secret_scope ON ep_secret(project_id,environment_id);
