-- Bug description: markdown body for defect details (rendered in the bug detail view).
ALTER TABLE ms_bug ADD COLUMN IF NOT EXISTS description TEXT;
