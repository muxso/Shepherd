-- 需求生命周期扩展:标签、父子关联、截止日期、创建/更新时间、开发与测试进度(含起止时间戳)。
-- 存量行取默认值;overdue 不落库,由应用按 due_date + 状态实时计算。
ALTER TABLE ms_requirement ADD COLUMN IF NOT EXISTS tags TEXT[] NOT NULL DEFAULT '{}';
ALTER TABLE ms_requirement ADD COLUMN IF NOT EXISTS parent_id TEXT;
ALTER TABLE ms_requirement ADD COLUMN IF NOT EXISTS due_date DATE;
ALTER TABLE ms_requirement ADD COLUMN IF NOT EXISTS created_at TIMESTAMPTZ NOT NULL DEFAULT now();
ALTER TABLE ms_requirement ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ NOT NULL DEFAULT now();
ALTER TABLE ms_requirement ADD COLUMN IF NOT EXISTS dev_status TEXT NOT NULL DEFAULT 'NOT_STARTED';
ALTER TABLE ms_requirement ADD COLUMN IF NOT EXISTS test_status TEXT NOT NULL DEFAULT 'NOT_STARTED';
ALTER TABLE ms_requirement ADD COLUMN IF NOT EXISTS dev_started_at TIMESTAMPTZ;
ALTER TABLE ms_requirement ADD COLUMN IF NOT EXISTS dev_finished_at TIMESTAMPTZ;
ALTER TABLE ms_requirement ADD COLUMN IF NOT EXISTS test_started_at TIMESTAMPTZ;
ALTER TABLE ms_requirement ADD COLUMN IF NOT EXISTS test_finished_at TIMESTAMPTZ;

CREATE INDEX IF NOT EXISTS ix_requirement_parent ON ms_requirement (parent_id);
