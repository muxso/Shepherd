-- 需求手工排序:列表内拖拽排序的显式秩。
-- 默认 0 → 未排序时回落到 seq(插入序),与历史行为一致;reorder 写入 1..N 显式秩。
ALTER TABLE ms_requirement ADD COLUMN IF NOT EXISTS sort_order BIGINT NOT NULL DEFAULT 0;

CREATE INDEX IF NOT EXISTS idx_requirement_order
    ON ms_requirement (project_id, sort_order, seq) WHERE deleted = false;
