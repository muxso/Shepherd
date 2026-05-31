-- api-test:资源池 / 项目接口配置 / 批量运行报告。
CREATE TABLE ms_resource_pool (
    id      TEXT PRIMARY KEY,
    name    TEXT NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT true,
    deleted BOOLEAN NOT NULL DEFAULT false
);

CREATE TABLE ms_project_api_config (
    project_id      TEXT PRIMARY KEY,
    default_pool_id TEXT
);

CREATE TABLE ms_api_batch_report (
    id         TEXT PRIMARY KEY DEFAULT gen_random_uuid()::text,
    pool_id    TEXT NOT NULL,
    run_mode   TEXT NOT NULL,
    case_count INT  NOT NULL,
    status     TEXT NOT NULL DEFAULT 'PENDING'
);
