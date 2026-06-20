-- 远程执行历史:每次「中央派给 agent 就地执行」的结果存档,供审计与下钻。
CREATE TABLE ms_runner_execution (
    id          text PRIMARY KEY DEFAULT gen_random_uuid()::text,
    agent_id    text NOT NULL,
    method      text NOT NULL,
    url         text NOT NULL,
    outcome     text NOT NULL,
    status      integer,
    elapsed_ms  bigint,
    failures    jsonb NOT NULL DEFAULT '[]'::jsonb,
    executed_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX ix_runner_execution_agent ON ms_runner_execution (agent_id, executed_at DESC);
