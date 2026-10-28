-- Source data is intentionally isolated from native requirements, delivery and AI ROI.
CREATE TABLE ms_devops_source (
 id TEXT PRIMARY KEY, project_id TEXT NOT NULL REFERENCES ms_project(id),
 cloud_org_id TEXT NOT NULL, cloud_project_id TEXT NOT NULL, name TEXT NOT NULL,
 last_success_at TIMESTAMPTZ, code_since TIMESTAMPTZ, code_until TIMESTAMPTZ,
 UNIQUE(cloud_org_id,cloud_project_id)
);
CREATE TABLE ms_devops_sync_run (
 id TEXT PRIMARY KEY, source_id TEXT NOT NULL REFERENCES ms_devops_source(id),
 status TEXT NOT NULL CHECK(status IN ('RUNNING','SUCCESS','FAILED')),
 started_at TIMESTAMPTZ NOT NULL DEFAULT now(), finished_at TIMESTAMPTZ,
 error TEXT NOT NULL DEFAULT '', counts JSONB NOT NULL DEFAULT '{}'
);
CREATE UNIQUE INDEX ux_devops_running ON ms_devops_sync_run(source_id) WHERE status='RUNNING';
CREATE TABLE ms_devops_stage (
 run_id TEXT NOT NULL REFERENCES ms_devops_sync_run(id) ON DELETE CASCADE,
 kind TEXT NOT NULL CHECK(kind IN ('requirement','bug','sprint','commit')),
 external_id TEXT NOT NULL, payload JSONB NOT NULL,
 PRIMARY KEY(run_id,kind,external_id)
);
CREATE TABLE ms_devops_entity (
 source_id TEXT NOT NULL REFERENCES ms_devops_source(id),kind TEXT NOT NULL,
 external_id TEXT NOT NULL,payload JSONB NOT NULL,active BOOLEAN NOT NULL DEFAULT true,
 first_seen_at TIMESTAMPTZ NOT NULL DEFAULT now(),observed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
 PRIMARY KEY(source_id,kind,external_id)
);
CREATE INDEX ix_devops_kind ON ms_devops_entity(source_id,kind) WHERE active;
CREATE TABLE ms_devops_history (
 id BIGSERIAL PRIMARY KEY,source_id TEXT NOT NULL REFERENCES ms_devops_source(id),
 kind TEXT NOT NULL,external_id TEXT NOT NULL,old_status TEXT,new_status TEXT,
 observed_at TIMESTAMPTZ NOT NULL DEFAULT now(),source_changed_at TEXT
);
CREATE INDEX ix_devops_history ON ms_devops_history(source_id,observed_at);
