-- 接口定义增审计列:创建人 / 创建时间 / 更新时间。
-- created_by 存 user_id(可空串);时间默认 now(),更新动作由仓储显式 bump updated_at。
ALTER TABLE ms_api_definition ADD COLUMN IF NOT EXISTS created_by TEXT NOT NULL DEFAULT '';
ALTER TABLE ms_api_definition ADD COLUMN IF NOT EXISTS created_at TIMESTAMPTZ NOT NULL DEFAULT now();
ALTER TABLE ms_api_definition ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ NOT NULL DEFAULT now();
