-- 用例评审队列:给评审头补 项目归属 / 用例集 / 创建时间 / 软删,支撑 创建·列表·详情。
ALTER TABLE ms_case_review ADD COLUMN IF NOT EXISTS project_id TEXT NOT NULL DEFAULT '';
ALTER TABLE ms_case_review ADD COLUMN IF NOT EXISTS case_ids TEXT[] NOT NULL DEFAULT '{}';
ALTER TABLE ms_case_review ADD COLUMN IF NOT EXISTS created_at TIMESTAMPTZ NOT NULL DEFAULT now();
ALTER TABLE ms_case_review ADD COLUMN IF NOT EXISTS deleted BOOLEAN NOT NULL DEFAULT false;
CREATE INDEX IF NOT EXISTS ix_case_review_project ON ms_case_review (project_id) WHERE NOT deleted;
