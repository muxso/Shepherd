ALTER TABLE ms_rag_document ADD COLUMN code_repository_id TEXT;
ALTER TABLE ms_rag_document ADD COLUMN code_branch TEXT;
ALTER TABLE ms_rag_document ADD COLUMN code_path TEXT;
ALTER TABLE ms_rag_document ADD COLUMN code_commit TEXT;
ALTER TABLE ms_rag_document ADD COLUMN code_blob TEXT;
UPDATE ms_rag_document SET
 code_repository_id=(source_id::jsonb)->>'repositoryId',code_branch=(source_id::jsonb)->>'branch',
 code_path=(source_id::jsonb)->>'path',code_commit=(source_id::jsonb)->>'commit',code_blob=(source_id::jsonb)->>'blob'
WHERE source_type='code' AND CASE WHEN pg_input_is_valid(source_id,'jsonb') THEN jsonb_typeof(source_id::jsonb)='object' ELSE false END;
CREATE INDEX ix_rag_code_repository ON ms_rag_document(project_id,code_repository_id,code_branch) WHERE source_type='code';
CREATE TABLE ms_rag_repository_checkpoint (
 project_id TEXT NOT NULL REFERENCES ms_project(id), repository_id TEXT NOT NULL,
 branch TEXT NOT NULL, commit_sha TEXT NOT NULL, name TEXT NOT NULL, url TEXT NOT NULL DEFAULT '',
 files JSONB NOT NULL, owner_id TEXT NOT NULL, visibility_groups TEXT[] NOT NULL DEFAULT '{}',
 updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
 PRIMARY KEY(project_id,repository_id,branch)
);
CREATE TABLE ms_requirement_review_action (
 id TEXT PRIMARY KEY DEFAULT gen_random_uuid()::text,
 review_id TEXT NOT NULL REFERENCES ms_requirement_impact_review(id),
 project_id TEXT NOT NULL REFERENCES ms_project(id), requirement_id TEXT NOT NULL REFERENCES ms_requirement(id),
 title TEXT NOT NULL, description TEXT NOT NULL, evidence_ids TEXT[] NOT NULL DEFAULT '{}',
 kind TEXT NOT NULL CHECK(kind IN ('QUESTION','CHANGE','TEST_DESIGN','REGRESSION')),
 status TEXT NOT NULL DEFAULT 'OPEN' CHECK(status IN ('OPEN','IN_PROGRESS','DONE')),
 assignee_id TEXT, functional_case_id TEXT REFERENCES ms_functional_case(id),
 created_by TEXT NOT NULL, created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
 updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
ALTER TABLE ms_test_inventory ADD COLUMN source_document_ids TEXT[] NOT NULL DEFAULT '{}';
ALTER TABLE ms_test_inventory ADD COLUMN file_blobs JSONB NOT NULL DEFAULT '{}';
CREATE TABLE ms_rag_git_commit (
 project_id TEXT NOT NULL REFERENCES ms_project(id), repository_id TEXT NOT NULL,
 sha TEXT NOT NULL, data JSONB NOT NULL, owner_id TEXT NOT NULL,
 visibility_groups TEXT[] NOT NULL DEFAULT '{}', updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
 PRIMARY KEY(project_id,repository_id,sha)
);
ALTER TABLE ms_requirement_impact_review ADD COLUMN heartbeat_at TIMESTAMPTZ NOT NULL DEFAULT now();

ALTER TABLE ms_requirement_review_action ADD COLUMN actual_result JSONB;
ALTER TABLE ms_requirement_impact_review ADD COLUMN decision JSONB;
