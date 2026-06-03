-- 环境:项目级,承载接口/用例/场景执行时的 base_url + 默认请求头 + 变量。
-- 运行时按 environment_id 选择(同一套用例可在多套环境跑);headers/variables 以 JSONB 存。
CREATE TABLE ms_environment (
    id         TEXT PRIMARY KEY DEFAULT gen_random_uuid()::text,
    project_id TEXT NOT NULL,
    name       TEXT NOT NULL,
    base_url   TEXT NOT NULL DEFAULT '',
    headers    JSONB NOT NULL DEFAULT '[]',   -- [{"name":"Authorization","value":"Bearer x"}]
    variables  JSONB NOT NULL DEFAULT '{}',   -- {"host":"localhost"}
    enabled    BOOLEAN NOT NULL DEFAULT true,
    deleted    BOOLEAN NOT NULL DEFAULT false
);
CREATE INDEX ix_environment_project ON ms_environment (project_id);

-- 项目默认环境(可选,运行入口未显式传 environmentId 时回退)。
ALTER TABLE ms_project_api_config ADD COLUMN IF NOT EXISTS default_environment_id TEXT;
