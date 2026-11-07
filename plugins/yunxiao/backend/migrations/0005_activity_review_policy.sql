CREATE TABLE ms_devops_activity_review_policy (
 source_id TEXT PRIMARY KEY REFERENCES ms_devops_source(id) ON DELETE CASCADE,
 policy JSONB NOT NULL CHECK(jsonb_typeof(policy)='object')
);
