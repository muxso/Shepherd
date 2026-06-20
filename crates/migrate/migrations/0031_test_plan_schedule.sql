-- 测试计划定时调度 + 定时运行快照。
-- schedule:为计划配 cron(6 段);run:每次到点为计划拍的统计快照(看通过率/执行率趋势)。
CREATE TABLE ms_test_plan_schedule (
    id       text PRIMARY KEY DEFAULT gen_random_uuid()::text,
    plan_id  text NOT NULL,
    cron     text NOT NULL,
    enabled  boolean NOT NULL DEFAULT true,
    deleted  boolean NOT NULL DEFAULT false
);

CREATE TABLE ms_test_plan_run (
    id           text PRIMARY KEY DEFAULT gen_random_uuid()::text,
    plan_id      text NOT NULL,
    status       text NOT NULL,
    total        bigint NOT NULL DEFAULT 0,
    pass_rate    double precision NOT NULL DEFAULT 0,
    execute_rate double precision NOT NULL DEFAULT 0,
    triggered_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX ix_test_plan_run_plan ON ms_test_plan_run (plan_id, triggered_at DESC);
