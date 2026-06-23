-- 任务负责人(人机协同):assignee 存负责人 id/名,assignee_kind 区分 HUMAN / AGENT;空 = 未分配。
ALTER TABLE ms_task ADD COLUMN IF NOT EXISTS assignee TEXT NOT NULL DEFAULT '';
ALTER TABLE ms_task ADD COLUMN IF NOT EXISTS assignee_kind TEXT NOT NULL DEFAULT '';
