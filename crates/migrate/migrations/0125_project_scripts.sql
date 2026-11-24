CREATE TABLE ms_project_script (
    id TEXT PRIMARY KEY DEFAULT gen_random_uuid()::text,
    project_id TEXT NOT NULL REFERENCES ms_project(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    language TEXT NOT NULL CHECK (language IN ('python', 'javascript')),
    revision INTEGER NOT NULL DEFAULT 1,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX ux_project_script_name ON ms_project_script(project_id, lower(name));

CREATE TABLE ms_project_script_revision (
    script_id TEXT NOT NULL REFERENCES ms_project_script(id) ON DELETE CASCADE,
    revision INTEGER NOT NULL,
    name TEXT NOT NULL,
    source TEXT NOT NULL,
    created_by TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (script_id, revision)
);
