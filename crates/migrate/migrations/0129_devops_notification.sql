CREATE TABLE ms_devops_notification (
    source_id text PRIMARY KEY,
    webhook_url text NOT NULL,
    rules jsonb NOT NULL DEFAULT '[]',
    revision bigint NOT NULL DEFAULT 1
);

CREATE TABLE ms_devops_notification_run (
    source_id text NOT NULL REFERENCES ms_devops_notification(source_id) ON DELETE CASCADE,
    rule_id uuid NOT NULL,
    local_date date NOT NULL,
    started_at timestamptz NOT NULL DEFAULT now(),
    outcome text NOT NULL,
    PRIMARY KEY (source_id, rule_id, local_date)
);
