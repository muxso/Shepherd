-- 需求增创建人列(对标 MeterSphere 需求列表「创建人」)。
-- created_by 存 user_id(创建时由 HTTP 层从会话注入;MCP/系统创建留空串);
-- 历史行回填空串,前端渲染为「—」。创建人不可变,仅 insert 时写入。
ALTER TABLE ms_requirement ADD COLUMN IF NOT EXISTS created_by TEXT NOT NULL DEFAULT '';
