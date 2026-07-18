-- Scenario cron schedules: one row per scenario, edited in batch from the list.
-- last_run_at defaults to now() so a newly saved schedule fires at the NEXT
-- cron occurrence instead of immediately.

CREATE TABLE IF NOT EXISTS ms_api_scenario_schedule (
    scenario_id text PRIMARY KEY,
    project_id text NOT NULL,
    cron text NOT NULL,
    env_mode text NOT NULL DEFAULT 'DEFAULT',
    env_id text,
    pool_id text,
    enabled boolean NOT NULL DEFAULT true,
    last_run_at timestamptz NOT NULL DEFAULT now(),
    created_by text NOT NULL DEFAULT '',
    updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS ix_scenario_schedule_project
    ON ms_api_scenario_schedule (project_id);

CREATE INDEX IF NOT EXISTS ix_scenario_schedule_enabled
    ON ms_api_scenario_schedule (enabled) WHERE enabled;
