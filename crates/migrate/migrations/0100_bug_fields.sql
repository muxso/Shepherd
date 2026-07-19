-- Bug first-class columns: severity (P0..P3, mirrors case priority), handler,
-- plus update audit (updated_by / updated_at). Every mutation stamps the audit
-- pair; inserts seed updated_by from created_by. Existing rows backfill
-- updated_at from created_at (epoch ms); updated_by stays NULL.
ALTER TABLE ms_bug ADD COLUMN IF NOT EXISTS severity   TEXT;
ALTER TABLE ms_bug ADD COLUMN IF NOT EXISTS handler    TEXT;
ALTER TABLE ms_bug ADD COLUMN IF NOT EXISTS updated_by TEXT;
ALTER TABLE ms_bug ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ;

UPDATE ms_bug SET updated_at = to_timestamp(created_at / 1000.0) WHERE updated_at IS NULL;
