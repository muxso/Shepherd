-- Plugin-owned metadata only. No foreign keys, columns, or migrations in the host database.
CREATE TABLE ep_profile (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    plugin_id TEXT NOT NULL,
    name TEXT NOT NULL,
    config JSONB NOT NULL CHECK(jsonb_typeof(config)='object'),
    enabled BOOLEAN NOT NULL DEFAULT true,
    revision BIGINT NOT NULL DEFAULT 1 CHECK(revision > 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(project_id,id)
);
CREATE INDEX ix_ep_profile_project ON ep_profile(project_id,plugin_id);
CREATE TABLE ep_binding (
    project_id TEXT NOT NULL,
    environment_id TEXT NOT NULL,
    profile_id TEXT NOT NULL,
    slot TEXT NOT NULL CHECK(slot IN ('connection','pre','post','assert','host','display','cleanup')),
    priority INT NOT NULL DEFAULT 0,
    enabled BOOLEAN NOT NULL DEFAULT true,
    PRIMARY KEY(project_id,environment_id,slot,profile_id),
    FOREIGN KEY(project_id,profile_id) REFERENCES ep_profile(project_id,id) ON DELETE RESTRICT
);
CREATE INDEX ix_ep_binding_environment ON ep_binding(project_id,environment_id,slot,priority);
