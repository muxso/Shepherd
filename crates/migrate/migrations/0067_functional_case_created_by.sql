-- 功能用例审计列:创建人(功能用例列表「创建人」列)。
-- created_by 落 user_id(创建时由组装根传入);旧行为 NULL(前端显示「—」)。
ALTER TABLE ms_functional_case ADD COLUMN IF NOT EXISTS created_by TEXT;
