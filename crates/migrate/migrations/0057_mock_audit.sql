-- Mock 审计列:操作人 + 更新时间(供「MOCK 视图」列)。旧行 created_by 空、updated_at 取 now()。
ALTER TABLE ms_api_mock ADD COLUMN IF NOT EXISTS created_by TEXT NOT NULL DEFAULT '';
ALTER TABLE ms_api_mock ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ NOT NULL DEFAULT now();
