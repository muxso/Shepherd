-- 接口定义域:接口定义(目录)+ 接口用例(关联定义,复用既有 ms_api_case 运行形态)+ Mock。
-- 接口类型 protocol:HTTP / TCP / SQL / DUBBO(本轮逻辑围绕 HTTP,枚举可扩展)。
-- 状态 status:DRAFT / DEBUGGING / COMPLETED / DEPRECATED。
CREATE TABLE ms_api_definition (
    id         TEXT PRIMARY KEY DEFAULT gen_random_uuid()::text,
    project_id TEXT NOT NULL,
    name       TEXT NOT NULL,
    protocol   TEXT NOT NULL DEFAULT 'HTTP',
    method     TEXT NOT NULL DEFAULT 'GET',
    path       TEXT NOT NULL DEFAULT '',
    status     TEXT NOT NULL DEFAULT 'DRAFT',
    deleted    BOOLEAN NOT NULL DEFAULT false
);
CREATE INDEX ix_api_definition_project ON ms_api_definition (project_id);

-- 接口用例关联到定义:扩展既有 ms_api_case(api-test runner 仍只读 method/url/body/assertions,
-- 故新增列均可空/带默认,既有批量执行链路不受影响)。让定义生成的用例可被现成 runner 执行。
ALTER TABLE ms_api_case ALTER COLUMN id SET DEFAULT gen_random_uuid()::text;
ALTER TABLE ms_api_case ADD COLUMN IF NOT EXISTS api_definition_id TEXT;
ALTER TABLE ms_api_case ADD COLUMN IF NOT EXISTS project_id TEXT;
ALTER TABLE ms_api_case ADD COLUMN IF NOT EXISTS name TEXT;
CREATE INDEX IF NOT EXISTS ix_api_case_definition ON ms_api_case (api_definition_id);

-- Mock:某接口定义的模拟期望。match_rule 以 JSONB 存匹配条件,response_* 为模拟响应。
CREATE TABLE ms_api_mock (
    id                TEXT PRIMARY KEY DEFAULT gen_random_uuid()::text,
    api_definition_id TEXT NOT NULL,
    name              TEXT NOT NULL,
    match_rule        JSONB NOT NULL DEFAULT '{}',
    response_status   INT  NOT NULL DEFAULT 200,
    response_body     TEXT,
    enabled           BOOLEAN NOT NULL DEFAULT true,
    deleted           BOOLEAN NOT NULL DEFAULT false
);
CREATE INDEX ix_api_mock_definition ON ms_api_mock (api_definition_id);
