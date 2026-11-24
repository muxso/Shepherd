ALTER TABLE ms_project_script_revision ADD COLUMN description TEXT NOT NULL DEFAULT '';
ALTER TABLE ms_project_script_revision ADD COLUMN tags TEXT[] NOT NULL DEFAULT '{}';
ALTER TABLE ms_project_script_revision ADD COLUMN parameters JSONB NOT NULL DEFAULT '[]';
ALTER TABLE ms_project_script_revision ADD COLUMN test_status TEXT CHECK (test_status IN ('passed', 'failed'));
