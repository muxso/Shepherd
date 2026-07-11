-- 需求 7 阶段流水线:每条需求固定 7 行(CREATED/AUDIT/REVIEW/DEV/TEST/ACCEPTANCE/DELIVERY)。
-- 旧的 dev_/test_ 列仍留在 ms_requirement 表里,但不再写入(被阶段行取代)。
CREATE TABLE IF NOT EXISTS ms_requirement_stage (
    requirement_id TEXT NOT NULL REFERENCES ms_requirement(id) ON DELETE CASCADE,
    stage          TEXT NOT NULL,
    status         TEXT NOT NULL DEFAULT 'PENDING',
    planned_start  DATE,
    planned_end    DATE,
    started_at     TIMESTAMPTZ,
    finished_at    TIMESTAMPTZ,
    PRIMARY KEY (requirement_id, stage)
);

-- 回填(幂等,ON CONFLICT DO NOTHING):为存量需求按旧列推导 7 行初始状态。
-- 创建即完成:起止都取 created_at。
INSERT INTO ms_requirement_stage (requirement_id, stage, status, started_at, finished_at)
SELECT id, 'CREATED', 'DONE', created_at, created_at FROM ms_requirement
ON CONFLICT DO NOTHING;

-- 审核是新阶段,存量一律待处理。
INSERT INTO ms_requirement_stage (requirement_id, stage, status)
SELECT id, 'AUDIT', 'PENDING' FROM ms_requirement
ON CONFLICT DO NOTHING;

-- 已定基线(或已交付)视为评审通过。
INSERT INTO ms_requirement_stage (requirement_id, stage, status)
SELECT id, 'REVIEW',
       CASE WHEN status IN ('BASELINED', 'DELIVERED') THEN 'DONE' ELSE 'PENDING' END
FROM ms_requirement
ON CONFLICT DO NOTHING;

-- 开发/测试:沿用旧工作状态(NOT_STARTED→PENDING)与起止时间。
INSERT INTO ms_requirement_stage (requirement_id, stage, status, started_at, finished_at)
SELECT id, 'DEV',
       CASE dev_status WHEN 'IN_PROGRESS' THEN 'IN_PROGRESS' WHEN 'DONE' THEN 'DONE' ELSE 'PENDING' END,
       dev_started_at, dev_finished_at
FROM ms_requirement
ON CONFLICT DO NOTHING;

INSERT INTO ms_requirement_stage (requirement_id, stage, status, started_at, finished_at)
SELECT id, 'TEST',
       CASE test_status WHEN 'IN_PROGRESS' THEN 'IN_PROGRESS' WHEN 'DONE' THEN 'DONE' ELSE 'PENDING' END,
       test_started_at, test_finished_at
FROM ms_requirement
ON CONFLICT DO NOTHING;

-- 已交付的需求视为验收/交付均完成。
INSERT INTO ms_requirement_stage (requirement_id, stage, status)
SELECT id, 'ACCEPTANCE',
       CASE WHEN status = 'DELIVERED' THEN 'DONE' ELSE 'PENDING' END
FROM ms_requirement
ON CONFLICT DO NOTHING;

INSERT INTO ms_requirement_stage (requirement_id, stage, status)
SELECT id, 'DELIVERY',
       CASE WHEN status = 'DELIVERED' THEN 'DONE' ELSE 'PENDING' END
FROM ms_requirement
ON CONFLICT DO NOTHING;
