-- 场景变更历史(审计日志):记录创建/更新/加步骤/删步骤/重排等操作(对齐 MeterSphere 变更历史)。
-- 由组装根在各变更操作成功后 best-effort 落一行;action 为操作类型,detail 为可读摘要。
CREATE TABLE ms_api_scenario_change (
    id          TEXT PRIMARY KEY DEFAULT gen_random_uuid()::text,
    scenario_id TEXT NOT NULL,
    action      TEXT NOT NULL,   -- CREATE | UPDATE | ADD_STEP | DELETE_STEP | REORDER
    detail      TEXT,            -- 可读摘要
    user_id     TEXT,            -- 操作人
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX ix_scenario_change ON ms_api_scenario_change (scenario_id, created_at DESC);
