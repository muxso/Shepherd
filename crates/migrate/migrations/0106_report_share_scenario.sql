-- Scenario report shares remember the owning scenario so the public page can render the
-- same step-tree the in-app report shows (resolved via the token-guarded public scenario read).
ALTER TABLE ms_report_share ADD COLUMN IF NOT EXISTS scenario_id TEXT;
