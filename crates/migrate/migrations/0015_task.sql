-- task (Shepherd 任务拆分):需求版本 → 可独立交付的任务 DAG。
-- 三张表:拆分图(元信息)+ 任务 + 依赖边。

CREATE TABLE ms_task_decomposition (
    seq                  BIGSERIAL,
    id                   TEXT PRIMARY KEY DEFAULT gen_random_uuid()::text,
    requirement_id       TEXT NOT NULL,
    requirement_version  INTEGER NOT NULL,
    -- 每个需求版本至多一张拆分图。
    UNIQUE (requirement_id, requirement_version)
);

CREATE TABLE ms_task (
    seq                  BIGSERIAL,
    decomposition_id     TEXT NOT NULL REFERENCES ms_task_decomposition(id) ON DELETE CASCADE,
    id                   TEXT NOT NULL,                 -- 拆分图内本地 id(如 t1)
    title                TEXT NOT NULL,
    description          TEXT NOT NULL DEFAULT '',
    acceptance_criteria  TEXT[] NOT NULL DEFAULT '{}',
    status               TEXT NOT NULL DEFAULT 'PENDING',
    PRIMARY KEY (decomposition_id, id)
);

CREATE TABLE ms_task_dependency (
    decomposition_id     TEXT NOT NULL,
    task_id              TEXT NOT NULL,                 -- 依赖方
    depends_on           TEXT NOT NULL,                 -- 被依赖方(同图内本地 id)
    PRIMARY KEY (decomposition_id, task_id, depends_on),
    FOREIGN KEY (decomposition_id, task_id) REFERENCES ms_task(decomposition_id, id) ON DELETE CASCADE
);
