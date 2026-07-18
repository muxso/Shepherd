-- test-plan: mind-map planning doc + editable plan fields (description/tags/module/dates/switches).
-- planning holds the test-planning tree verbatim: {nodes:[{id,name,kind,children,config,caseIds,scenarioIds}]}.
ALTER TABLE ms_test_plan ADD COLUMN IF NOT EXISTS planning jsonb;
ALTER TABLE ms_test_plan ADD COLUMN IF NOT EXISTS description text NOT NULL DEFAULT '';
ALTER TABLE ms_test_plan ADD COLUMN IF NOT EXISTS tags jsonb NOT NULL DEFAULT '[]';
ALTER TABLE ms_test_plan ADD COLUMN IF NOT EXISTS module_id text;
ALTER TABLE ms_test_plan ADD COLUMN IF NOT EXISTS start_at timestamptz;
ALTER TABLE ms_test_plan ADD COLUMN IF NOT EXISTS end_at timestamptz;
ALTER TABLE ms_test_plan ADD COLUMN IF NOT EXISTS allow_duplicate_cases boolean NOT NULL DEFAULT true;
ALTER TABLE ms_test_plan ADD COLUMN IF NOT EXISTS auto_update_status boolean NOT NULL DEFAULT true;
