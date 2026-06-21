-- 任务 ↔ 接口用例 关联(打通 需求→拆分→任务→用例 链路)。
-- 任务 id 仅在其拆分图内唯一,故联合键含 decomposition_id。
CREATE TABLE ms_task_case (
    decomposition_id TEXT NOT NULL,
    task_id          TEXT NOT NULL,
    case_id          TEXT NOT NULL,
    UNIQUE (decomposition_id, task_id, case_id)
);
CREATE INDEX ix_task_case_task ON ms_task_case (decomposition_id, task_id);
CREATE INDEX ix_task_case_case ON ms_task_case (case_id);
