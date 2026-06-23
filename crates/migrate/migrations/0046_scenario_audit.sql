-- 场景审计列:创建人 / 创建时间 / 更新时间(对齐 MeterSphere 场景基本信息只读项 #21)。
-- created_by 落 user_id(创建时由组装根传入);更新时 updated_at 由 PATCH 回写 now()。
ALTER TABLE ms_api_scenario ADD COLUMN IF NOT EXISTS created_by TEXT;
ALTER TABLE ms_api_scenario ADD COLUMN IF NOT EXISTS created_at TIMESTAMPTZ NOT NULL DEFAULT now();
ALTER TABLE ms_api_scenario ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ NOT NULL DEFAULT now();
