CREATE TABLE IF NOT EXISTS ms_sonar_analysis (
    source_id TEXT NOT NULL,
    scope TEXT NOT NULL,
    task_id TEXT NOT NULL,
    repository_id TEXT NOT NULL,
    pull_request TEXT NOT NULL,
    revision TEXT NOT NULL,
    analyzed_at TIMESTAMPTZ NOT NULL,
    report JSONB NOT NULL,
    PRIMARY KEY (source_id, scope, task_id)
);
CREATE INDEX IF NOT EXISTS ix_sonar_analysis_source_time ON ms_sonar_analysis (source_id, analyzed_at DESC);

CREATE TABLE IF NOT EXISTS ms_sonar_notification_settings (
    source_id TEXT PRIMARY KEY,
    dashboard_base TEXT,
    webhook_url TEXT,
    message_template JSONB
);
