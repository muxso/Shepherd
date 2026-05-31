-- test-plan:计划(含计划组)+ 用例执行计数。
CREATE TABLE ms_test_plan (
    id             TEXT PRIMARY KEY DEFAULT gen_random_uuid()::text,
    project_id     TEXT NOT NULL,
    name           TEXT NOT NULL,
    plan_type      TEXT NOT NULL,                    -- TEST_PLAN | GROUP
    group_id       TEXT NOT NULL DEFAULT 'NONE',
    archived       BOOLEAN NOT NULL DEFAULT false,
    pass_threshold DOUBLE PRECISION NOT NULL DEFAULT 1.0
);
CREATE INDEX ix_test_plan_group ON ms_test_plan (group_id);

CREATE TABLE ms_test_plan_case_count (
    plan_id    TEXT PRIMARY KEY,
    pending    BIGINT NOT NULL DEFAULT 0,
    success    BIGINT NOT NULL DEFAULT 0,
    error      BIGINT NOT NULL DEFAULT 0,
    fake_error BIGINT NOT NULL DEFAULT 0,
    block      BIGINT NOT NULL DEFAULT 0
);
