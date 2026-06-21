-- 接口模块(文件夹):项目内可嵌套地分组接口定义。parent_id 为 NULL 即顶层模块。
CREATE TABLE ms_api_module (
    id         TEXT PRIMARY KEY DEFAULT gen_random_uuid()::text,
    project_id TEXT NOT NULL,
    parent_id  TEXT,
    name       TEXT NOT NULL,
    deleted    BOOLEAN NOT NULL DEFAULT false
);
CREATE INDEX ix_api_module_project ON ms_api_module (project_id) WHERE NOT deleted;

-- 接口定义归属模块(NULL = 未归类)。
ALTER TABLE ms_api_definition ADD COLUMN IF NOT EXISTS module_id TEXT;
CREATE INDEX IF NOT EXISTS ix_api_definition_module ON ms_api_definition (module_id) WHERE module_id IS NOT NULL;
