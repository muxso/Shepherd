-- 定时导入计划:给一个 URL 来源配 cron,后台调度器到点拉取并按格式导入接口。
-- 复用接口导入的全部选项(目标模块 / 按标签分组 / 覆盖 / 同步目录)。
CREATE TABLE IF NOT EXISTS ms_api_import_schedule (
    id           text PRIMARY KEY DEFAULT gen_random_uuid()::text,
    project_id   text NOT NULL,
    name         text NOT NULL DEFAULT '',
    format       text NOT NULL DEFAULT 'openapi',   -- openapi/postman/har/jmeter/metersphere
    source_url   text NOT NULL,
    auth_token   text NOT NULL DEFAULT '',          -- Authorization 头 token(明文)
    basic_auth   boolean NOT NULL DEFAULT false,    -- token 走 Basic(true)还是 Bearer(false)
    module_id    text,
    group_by_tag boolean NOT NULL DEFAULT true,
    overwrite    boolean NOT NULL DEFAULT true,
    sync_module  boolean NOT NULL DEFAULT false,
    cron         text NOT NULL,                     -- 6 段(秒 分 时 日 月 周)
    enabled      boolean NOT NULL DEFAULT true,
    last_run_at  timestamptz,                       -- 最近运行时间(NULL=未运行)
    last_result  text NOT NULL DEFAULT '',          -- 最近运行结果摘要
    created_by   text NOT NULL DEFAULT '',
    created_at   timestamptz NOT NULL DEFAULT now(),
    deleted      boolean NOT NULL DEFAULT false
);

CREATE INDEX IF NOT EXISTS idx_api_import_schedule_project
    ON ms_api_import_schedule (project_id) WHERE deleted = false;
