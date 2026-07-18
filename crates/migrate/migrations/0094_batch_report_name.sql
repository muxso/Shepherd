-- Display name for batch reports (set by scenario batch-run union reports).

ALTER TABLE ms_api_batch_report ADD COLUMN IF NOT EXISTS name text NOT NULL DEFAULT '';
