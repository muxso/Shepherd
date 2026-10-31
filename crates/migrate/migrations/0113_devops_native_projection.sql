-- Native assets retain source identity; only externally owned rows are mirrored.
ALTER TABLE ms_requirement ADD COLUMN external_source_id TEXT REFERENCES ms_devops_source(id);
ALTER TABLE ms_requirement ADD COLUMN external_id TEXT;
ALTER TABLE ms_requirement ADD COLUMN external_content_hash TEXT;
CREATE UNIQUE INDEX ux_requirement_external ON ms_requirement(external_source_id,external_id) WHERE external_source_id IS NOT NULL;
ALTER TABLE ms_functional_case ADD COLUMN external_source_id TEXT REFERENCES ms_devops_source(id);
ALTER TABLE ms_functional_case ADD COLUMN external_id TEXT;
CREATE UNIQUE INDEX ux_functional_case_external ON ms_functional_case(external_source_id,external_id) WHERE external_source_id IS NOT NULL;
ALTER TABLE ms_devops_stage DROP CONSTRAINT ms_devops_stage_kind_check;
ALTER TABLE ms_devops_stage ADD CONSTRAINT ms_devops_stage_kind_check CHECK(kind IN ('requirement','bug','sprint','commit','testcase'));
