-- 第三方身份 ↔ 本地用户映射(飞书/企业微信)。首次登录开通,之后复用同一 user_id。
CREATE TABLE ms_external_user_link (
    provider     TEXT NOT NULL,
    open_id      TEXT NOT NULL,
    user_id      TEXT NOT NULL DEFAULT gen_random_uuid()::text,
    display_name TEXT NOT NULL,
    permissions  TEXT[] NOT NULL DEFAULT '{}',
    PRIMARY KEY (provider, open_id)
);
