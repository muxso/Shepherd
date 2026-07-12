-- 需求/缺陷的自定义字段值:map<字段key, 字符串值>;字段定义由项目模板 ms_template 管理,多选值以逗号拼接。
ALTER TABLE ms_requirement ADD COLUMN IF NOT EXISTS custom_fields JSONB NOT NULL DEFAULT '{}'::jsonb;
ALTER TABLE ms_bug ADD COLUMN IF NOT EXISTS custom_fields JSONB NOT NULL DEFAULT '{}'::jsonb;
