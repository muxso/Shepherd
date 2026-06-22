-- 用例结果响应明细:状态码 / 耗时 / 响应大小 / 响应体 / 响应头。
-- 由执行器(PlanExecutor)在记录通过失败后 best-effort 回填(record_detail);
-- 旧报告/未回填行为 NULL。供场景报告逐步展开「响应体/响应头/状态码/耗时/大小」(对齐 #26)。
ALTER TABLE ms_api_case_result ADD COLUMN IF NOT EXISTS status_code INT;
ALTER TABLE ms_api_case_result ADD COLUMN IF NOT EXISTS latency_ms  BIGINT;
ALTER TABLE ms_api_case_result ADD COLUMN IF NOT EXISTS resp_size   BIGINT;
ALTER TABLE ms_api_case_result ADD COLUMN IF NOT EXISTS body        TEXT;
ALTER TABLE ms_api_case_result ADD COLUMN IF NOT EXISTS headers     JSONB;
