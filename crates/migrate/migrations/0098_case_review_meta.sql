-- Case review header meta: name / description / tags / module / schedule / creator for the list page.
-- (project_id / case_ids / created_at / deleted were added by 0055.)
ALTER TABLE ms_case_review ADD COLUMN IF NOT EXISTS name TEXT NOT NULL DEFAULT '';
ALTER TABLE ms_case_review ADD COLUMN IF NOT EXISTS description TEXT NOT NULL DEFAULT '';
ALTER TABLE ms_case_review ADD COLUMN IF NOT EXISTS tags JSONB NOT NULL DEFAULT '[]';
ALTER TABLE ms_case_review ADD COLUMN IF NOT EXISTS module_id TEXT;
ALTER TABLE ms_case_review ADD COLUMN IF NOT EXISTS start_at TIMESTAMPTZ;
ALTER TABLE ms_case_review ADD COLUMN IF NOT EXISTS end_at TIMESTAMPTZ;
ALTER TABLE ms_case_review ADD COLUMN IF NOT EXISTS created_by TEXT;
