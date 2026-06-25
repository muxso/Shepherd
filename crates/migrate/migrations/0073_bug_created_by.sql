-- 缺陷审计列:创建人(对齐其它实体的 created_by;bug pg 已 INSERT/RETURNING 此列)。
-- created_by 落 user_id(创建时由组装根传入);旧行为 NULL(前端显示「—」)。
ALTER TABLE ms_bug ADD COLUMN IF NOT EXISTS created_by TEXT;
