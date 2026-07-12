-- 个人中心:API Key 增加属主与有效期;新增每用户 LLM 模型配置表。
-- 旧数据 user_id 缺省空串(无属主,仅系统权限可管),expires_at 为 NULL(永久)。
ALTER TABLE ms_apikey
    ADD COLUMN IF NOT EXISTS user_id TEXT NOT NULL DEFAULT '',
    ADD COLUMN IF NOT EXISTS expires_at TIMESTAMPTZ;

CREATE TABLE IF NOT EXISTS ms_user_llm_model (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id    TEXT NOT NULL,
    provider   TEXT NOT NULL,        -- deepseek/openai/zhipu/custom 等,小写
    name       TEXT NOT NULL,        -- 模型名,如 deepseek-chat
    base_url   TEXT NOT NULL DEFAULT '',
    api_key    TEXT NOT NULL DEFAULT '',
    enabled    BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(user_id, provider, name)
);
