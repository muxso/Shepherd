-- 接口定义变更历史(审计):记录定义的创建 / 规格更新 / 模块移动等动作。
-- 追加写,永不更新;按 definition_id + created_at 倒序读。detail 存简短 JSON/文本描述。
CREATE TABLE IF NOT EXISTS ms_api_definition_change (
    id            TEXT PRIMARY KEY DEFAULT gen_random_uuid()::text,
    definition_id TEXT NOT NULL,
    action        TEXT NOT NULL,            -- CREATE | UPDATE_SPEC | MOVE_MODULE | RENAME | ...
    detail        TEXT NOT NULL DEFAULT '', -- 简短描述(可空串)
    actor         TEXT NOT NULL DEFAULT '', -- 操作者 user_id
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS ix_api_def_change_def ON ms_api_definition_change (definition_id, created_at DESC);
