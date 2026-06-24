-- 测试计划补记创建时间 + 按项目列表索引。
-- 列表端点 GET /test-plan?projectId= 需要按项目过滤并按创建时间倒序,
-- 取代前端 localStorage 注册表(registry.ts)维护的「假列表」。
ALTER TABLE ms_test_plan ADD COLUMN IF NOT EXISTS created_at timestamptz NOT NULL DEFAULT now();
CREATE INDEX IF NOT EXISTS ix_test_plan_project ON ms_test_plan (project_id, created_at DESC);
