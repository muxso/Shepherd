CREATE TABLE ms_devops_sonar_analysis (
    source_id TEXT NOT NULL REFERENCES ms_devops_source(id),
    scope TEXT NOT NULL CHECK (scope IN ('sonar_repository', 'ci_pull_request')),
    task_id TEXT NOT NULL,
    repository_id TEXT NOT NULL,
    pull_request TEXT NOT NULL,
    revision TEXT NOT NULL,
    analyzed_at TIMESTAMPTZ NOT NULL,
    report JSONB NOT NULL,
    CHECK ((scope = 'sonar_repository' AND pull_request = '') OR
           (scope = 'ci_pull_request' AND pull_request <> '')),
    PRIMARY KEY (source_id, scope, task_id)
);

CREATE INDEX ix_devops_sonar_latest
    ON ms_devops_sonar_analysis (source_id, scope, repository_id, pull_request, analyzed_at DESC, task_id DESC);

CREATE TRIGGER statistics_sonar_insert
    AFTER INSERT ON ms_devops_sonar_analysis
    REFERENCING NEW TABLE AS new_rows
    FOR EACH STATEMENT EXECUTE FUNCTION ms_devops_invalidate_statistics();
