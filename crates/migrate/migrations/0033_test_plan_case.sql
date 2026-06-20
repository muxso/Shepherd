-- 测试计划 ↔ 用例 关联 + 逐用例执行结果(报告逐条明细的真源)。
-- status: PENDING/SUCCESS/ERROR/FAKE_ERROR/BLOCK;result 存执行明细(耗时/大小/状态码/断言/响应体)。
-- 计划统计(通过率/执行率)改由本表按 status 聚合,取代手填的 ms_test_plan_case_count。
CREATE TABLE ms_test_plan_case (
    id          text PRIMARY KEY DEFAULT gen_random_uuid()::text,
    plan_id     text NOT NULL,
    case_id     text NOT NULL,
    name        text NOT NULL DEFAULT '',
    status      text NOT NULL DEFAULT 'PENDING',
    result      jsonb,
    executed_at timestamptz,
    UNIQUE (plan_id, case_id)
);
CREATE INDEX idx_test_plan_case_plan ON ms_test_plan_case (plan_id);
