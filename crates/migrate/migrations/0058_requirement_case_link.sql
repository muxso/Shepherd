-- 需求 ↔ 功能用例 覆盖关联(按验收标准下标 criterion_index)。多对多:
--   一条标准可被多条功能用例覆盖;一条功能用例可覆盖多条标准/多个需求。
-- 打通「需求 → 验收标准 → 功能用例」覆盖链(手工测试侧),与 ms_task_case(任务↔接口用例)并列。
CREATE TABLE IF NOT EXISTS ms_requirement_case (
    requirement_id     TEXT    NOT NULL,
    criterion_index    INTEGER NOT NULL,
    functional_case_id TEXT    NOT NULL,
    project_id         TEXT    NOT NULL DEFAULT '',
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (requirement_id, criterion_index, functional_case_id)
);
CREATE INDEX IF NOT EXISTS ix_req_case_case ON ms_requirement_case (functional_case_id);
CREATE INDEX IF NOT EXISTS ix_req_case_req ON ms_requirement_case (requirement_id);
