-- DB-backed OIDC provider configuration. Replaces the env-var-only
-- feishu/wecom wiring with a runtime-editable table covering Feishu, Lark,
-- WeCom, DingTalk and Slack. The runtime registry is rebuilt from the enabled
-- rows on startup and on every admin mutation.
CREATE TABLE IF NOT EXISTS ms_oidc_provider (
    provider_key        TEXT PRIMARY KEY,
    app_id              TEXT NOT NULL,
    app_secret          TEXT NOT NULL,
    redirect            TEXT NOT NULL DEFAULT '',
    default_permissions TEXT[] NOT NULL DEFAULT '{}',
    enabled             BOOLEAN NOT NULL DEFAULT TRUE,
    base_url            TEXT
);
