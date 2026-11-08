CREATE TABLE ms_test_service (
 id TEXT PRIMARY KEY DEFAULT gen_random_uuid()::text,
 project_id TEXT NOT NULL REFERENCES ms_project(id),
 name TEXT NOT NULL, description TEXT NOT NULL DEFAULT '',
 repositories JSONB NOT NULL DEFAULT '[]',
 deleted BOOLEAN NOT NULL DEFAULT false,
 updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX ux_test_service_name ON ms_test_service(project_id,name) WHERE NOT deleted;
CREATE TABLE ms_test_source_mapping (
 id TEXT PRIMARY KEY DEFAULT gen_random_uuid()::text,
 project_id TEXT NOT NULL REFERENCES ms_project(id),
 source_id TEXT NOT NULL,
 scope TEXT NOT NULL CHECK(scope IN ('case','directory','library')),
 external_id TEXT NOT NULL,
 service_id TEXT NOT NULL REFERENCES ms_test_service(id),
 UNIQUE(project_id,source_id,scope,external_id)
);
CREATE TABLE ms_test_case_service (
 case_id TEXT PRIMARY KEY REFERENCES ms_functional_case(id),
 project_id TEXT NOT NULL REFERENCES ms_project(id),
 service_id TEXT NOT NULL REFERENCES ms_test_service(id)
);
CREATE TABLE ms_test_asset_origin (
 project_id TEXT NOT NULL REFERENCES ms_project(id),
 provider TEXT NOT NULL, source_id TEXT NOT NULL,
 kind TEXT NOT NULL CHECK(kind IN ('testcase','requirement')),
 external_id TEXT NOT NULL, native_id TEXT NOT NULL,
 source_payload JSONB NOT NULL, native_snapshot JSONB NOT NULL,
 source_revision TEXT NOT NULL, keep_local BOOLEAN NOT NULL DEFAULT false, source_present BOOLEAN NOT NULL DEFAULT true,
 synced_at TIMESTAMPTZ NOT NULL DEFAULT now(),
 PRIMARY KEY(project_id,provider,source_id,kind,external_id),
 UNIQUE(kind,native_id)
);
CREATE TABLE ms_test_import (
 id TEXT PRIMARY KEY DEFAULT gen_random_uuid()::text,
 project_id TEXT NOT NULL REFERENCES ms_project(id), source_id TEXT NOT NULL,
 source_revision TEXT NOT NULL, records JSONB NOT NULL, preview JSONB NOT NULL,
 status TEXT NOT NULL DEFAULT 'PREVIEW' CHECK(status IN ('PREVIEW','APPLIED')),
 created_by TEXT NOT NULL, created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
 applied_at TIMESTAMPTZ, result JSONB
);
CREATE INDEX ix_test_import_project ON ms_test_import(project_id,created_at DESC);
CREATE TABLE ms_functional_scenario (
 id TEXT PRIMARY KEY DEFAULT gen_random_uuid()::text,
 project_id TEXT NOT NULL REFERENCES ms_project(id), name TEXT NOT NULL,
 description TEXT NOT NULL DEFAULT '', preconditions TEXT NOT NULL DEFAULT '',
 steps JSONB NOT NULL, definition_snapshot JSONB NOT NULL DEFAULT '{}', requirement_ids TEXT[] NOT NULL DEFAULT '{}',
 api_scenario_id TEXT,
 status TEXT NOT NULL DEFAULT 'DRAFT' CHECK(status IN ('DRAFT','REVIEWED')),
 revision INTEGER NOT NULL DEFAULT 1,
 deleted BOOLEAN NOT NULL DEFAULT false, created_by TEXT NOT NULL,
 updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE TABLE ms_functional_scenario_run (
 id TEXT PRIMARY KEY DEFAULT gen_random_uuid()::text,
 scenario_id TEXT NOT NULL REFERENCES ms_functional_scenario(id),
 project_id TEXT NOT NULL REFERENCES ms_project(id), revision INTEGER NOT NULL,
 snapshot JSONB NOT NULL, steps JSONB NOT NULL,
 environment TEXT NOT NULL, outcome TEXT NOT NULL CHECK(outcome IN ('PASSED','FAILED','BLOCKED')),
 notes TEXT NOT NULL DEFAULT '', executed_by TEXT NOT NULL,
 executed_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE TABLE ms_test_inventory (
 id TEXT PRIMARY KEY DEFAULT gen_random_uuid()::text,
 service_id TEXT NOT NULL REFERENCES ms_test_service(id),
 project_id TEXT NOT NULL REFERENCES ms_project(id),
 repository_id TEXT NOT NULL, branch TEXT NOT NULL, commit_sha TEXT NOT NULL,
 state TEXT NOT NULL CHECK(state IN ('PARTIAL','REVIEWED','FAILED')),
 endpoints JSONB NOT NULL DEFAULT '[]', limitations JSONB NOT NULL DEFAULT '[]',
 scanned_paths JSONB NOT NULL DEFAULT '[]',
 reviewed_by TEXT, created_by TEXT NOT NULL,
 created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX ix_test_inventory_service ON ms_test_inventory(project_id,service_id,created_at DESC);
CREATE TABLE ms_requirement_impact_review (
 id TEXT PRIMARY KEY DEFAULT gen_random_uuid()::text,
 project_id TEXT NOT NULL REFERENCES ms_project(id),
 requirement_id TEXT NOT NULL REFERENCES ms_requirement(id), requirement_version INTEGER NOT NULL,
 state TEXT NOT NULL CHECK(state IN ('QUEUED','RUNNING','COMPLETED','FAILED','CANCELLED')),
 baseline JSONB NOT NULL, result JSONB, error TEXT,
 created_by TEXT NOT NULL, created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
 finished_at TIMESTAMPTZ
);
CREATE INDEX ix_requirement_impact_review ON ms_requirement_impact_review(project_id,requirement_id,created_at DESC);
