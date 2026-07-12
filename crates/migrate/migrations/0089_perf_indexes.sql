-- Performance indexes for hot list/lookup paths.
CREATE INDEX IF NOT EXISTS ix_requirement_project ON ms_requirement (project_id) WHERE NOT deleted;
CREATE INDEX IF NOT EXISTS ix_task_verified_at ON ms_task (verified_at) WHERE verified_at IS NOT NULL;
CREATE INDEX IF NOT EXISTS ix_api_case_project ON ms_api_case (project_id);
CREATE INDEX IF NOT EXISTS ix_apikey_user ON ms_apikey (user_id);
CREATE INDEX IF NOT EXISTS ix_session_expires ON ms_session (expires_at);
