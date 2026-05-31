-- verification (Shepherd 完整性验证):需求 ←→ 任务 ←→ 实现 双向追溯 + 缺口检测。
CREATE TABLE ms_verification (
    seq                  BIGSERIAL,
    id                   TEXT PRIMARY KEY DEFAULT gen_random_uuid()::text,
    requirement_id       TEXT NOT NULL,
    requirement_version  INTEGER NOT NULL,
    UNIQUE (requirement_id, requirement_version)   -- 每个需求版本一个验证
);

-- 验收标准快照(0-based idx)。
CREATE TABLE ms_verification_criterion (
    verification_id  TEXT NOT NULL REFERENCES ms_verification(id) ON DELETE CASCADE,
    idx              INTEGER NOT NULL,
    text             TEXT NOT NULL,
    PRIMARY KEY (verification_id, idx)
);

-- 覆盖链:criterion ↔ task;satisfied = 该任务已交付且验证。
CREATE TABLE ms_verification_link (
    verification_id   TEXT NOT NULL REFERENCES ms_verification(id) ON DELETE CASCADE,
    criterion_idx     INTEGER NOT NULL,
    decomposition_id  TEXT NOT NULL,
    task_id           TEXT NOT NULL,
    satisfied         BOOLEAN NOT NULL DEFAULT false,
    PRIMARY KEY (verification_id, criterion_idx, decomposition_id, task_id)
);
