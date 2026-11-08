CREATE TABLE ms_test_case_api (
 id TEXT PRIMARY KEY DEFAULT gen_random_uuid()::text,
 project_id TEXT NOT NULL REFERENCES ms_project(id),
 case_id TEXT NOT NULL REFERENCES ms_functional_case(id),
 api_definition_id TEXT REFERENCES ms_api_definition(id),
 api_case_id TEXT REFERENCES ms_api_case(id),
 CHECK(api_definition_id IS NOT NULL OR api_case_id IS NOT NULL)
);
CREATE UNIQUE INDEX ux_test_case_api ON ms_test_case_api(case_id,COALESCE(api_definition_id,''),COALESCE(api_case_id,''));
CREATE INDEX ix_test_case_api_project ON ms_test_case_api(project_id,case_id);
