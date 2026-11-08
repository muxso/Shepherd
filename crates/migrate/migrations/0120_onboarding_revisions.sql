CREATE TABLE ms_rag_onboarding_revision (
 document_id TEXT NOT NULL REFERENCES ms_rag_document(id) ON DELETE CASCADE,
 version INTEGER NOT NULL,
 project_id TEXT NOT NULL,
 title TEXT NOT NULL,
 content TEXT NOT NULL,
 source_url TEXT NOT NULL DEFAULT '',
 visibility_groups TEXT[] NOT NULL DEFAULT '{}',
 updated_by TEXT NOT NULL,
 updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
 PRIMARY KEY(document_id,version)
);
CREATE INDEX ix_rag_onboarding_project ON ms_rag_onboarding_revision(project_id,updated_at DESC);
