-- 场景域:场景(编排)+ 有序步骤。
-- 步骤 kind:REQUEST(内联请求)/ CASE(引用接口用例)/ SCENARIO(引用子场景,可嵌套)。
-- ref_mode:REFERENCE(活链,执行时取最新)/ COPY(快照,加步骤时固化到 inline)。
-- 状态 status:DRAFT / DEBUGGING / COMPLETED / DEPRECATED。
CREATE TABLE ms_api_scenario (
    id         TEXT PRIMARY KEY DEFAULT gen_random_uuid()::text,
    project_id TEXT NOT NULL,
    name       TEXT NOT NULL,
    status     TEXT NOT NULL DEFAULT 'DRAFT',
    deleted    BOOLEAN NOT NULL DEFAULT false
);
CREATE INDEX ix_api_scenario_project ON ms_api_scenario (project_id);

CREATE TABLE ms_api_scenario_step (
    id          TEXT PRIMARY KEY DEFAULT gen_random_uuid()::text,
    scenario_id TEXT NOT NULL,
    step_order  INT  NOT NULL,
    kind        TEXT NOT NULL,                     -- REQUEST | CASE | SCENARIO
    ref_mode    TEXT NOT NULL DEFAULT 'REFERENCE', -- REFERENCE | COPY
    ref_id      TEXT,                              -- CASE→case_id / SCENARIO→scenario_id
    inline      JSONB                              -- REQUEST 的内联请求,或 COPY 的快照
);
CREATE INDEX ix_api_scenario_step_scenario ON ms_api_scenario_step (scenario_id);
