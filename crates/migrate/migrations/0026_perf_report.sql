-- 压测报告:一次 `perf run` 的目标 + 计划 + 聚合结果(吞吐/错误率/延迟分位)。
-- 状态 RUNNING(后台执行中)→ COMPLETED(跑完,error_rate 表征质量)/ ERROR(执行异常)。
CREATE TABLE ms_perf_report (
    id             text PRIMARY KEY DEFAULT gen_random_uuid()::text,
    project_id     text NOT NULL DEFAULT '',
    method         text NOT NULL,
    url            text NOT NULL,
    concurrency    integer NOT NULL,
    iterations     integer NOT NULL,
    status         text NOT NULL DEFAULT 'RUNNING',
    total          integer NOT NULL DEFAULT 0,
    success        integer NOT NULL DEFAULT 0,
    failed         integer NOT NULL DEFAULT 0,
    error_rate     double precision NOT NULL DEFAULT 0,
    throughput_rps double precision NOT NULL DEFAULT 0,
    elapsed_ms     bigint NOT NULL DEFAULT 0,
    latency        jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at     timestamptz NOT NULL DEFAULT now()
);
