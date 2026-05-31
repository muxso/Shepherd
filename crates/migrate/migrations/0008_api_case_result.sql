-- api-test:批量运行的 per-case 结果明细。
-- outcome: SUCCESS | ERROR;failures 以 JSONB 存失败原因字符串数组。
CREATE TABLE ms_api_case_result (
    report_id TEXT NOT NULL,
    case_id   TEXT NOT NULL,
    outcome   TEXT NOT NULL,
    failures  JSONB NOT NULL DEFAULT '[]',
    PRIMARY KEY (report_id, case_id)
);
CREATE INDEX ix_api_case_result_report ON ms_api_case_result (report_id);
