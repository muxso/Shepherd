-- test-plan: creator user id, shown in the plan list.
ALTER TABLE ms_test_plan ADD COLUMN IF NOT EXISTS created_by text;
