-- 接口测试执行记录:用例执行明细补时间戳/项目(分页排序+过滤),新增场景执行记录表。

-- 用例执行明细(ms_api_case_result 已存在:report_id, case_id, outcome, failures)。
-- 补 executed_at(分页按时间倒序)与 project_id(按项目过滤);均带默认/可空,既有 runner 写入不受影响。
ALTER TABLE ms_api_case_result ADD COLUMN IF NOT EXISTS executed_at TIMESTAMPTZ NOT NULL DEFAULT now();
ALTER TABLE ms_api_case_result ADD COLUMN IF NOT EXISTS project_id TEXT;
CREATE INDEX IF NOT EXISTS ix_api_case_result_case ON ms_api_case_result (case_id, executed_at DESC);

-- 场景执行记录:每次运行场景产生一条,带执行状态。
-- status:PENDING(已派发待跑)/ RUNNING / SUCCESS / ERROR。
CREATE TABLE ms_api_scenario_execution (
    id          TEXT PRIMARY KEY DEFAULT gen_random_uuid()::text,
    scenario_id TEXT NOT NULL,
    project_id  TEXT NOT NULL,
    status      TEXT NOT NULL DEFAULT 'PENDING',
    case_count  INT  NOT NULL DEFAULT 0,
    report_id   TEXT,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX ix_scenario_exec_scenario ON ms_api_scenario_execution (scenario_id, created_at DESC);
CREATE INDEX ix_scenario_exec_project ON ms_api_scenario_execution (project_id, created_at DESC);
