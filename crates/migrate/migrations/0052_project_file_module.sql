-- 项目文件归属模块(NULL = 未规划)。复用 ms_api_module 作为项目级模块树。
ALTER TABLE ms_project_file ADD COLUMN IF NOT EXISTS module_id TEXT;
CREATE INDEX IF NOT EXISTS ix_project_file_module ON ms_project_file (module_id) WHERE module_id IS NOT NULL;
