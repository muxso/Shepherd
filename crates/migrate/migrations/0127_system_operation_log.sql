CREATE TABLE ms_system_operation_log (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    recorded_at timestamptz NOT NULL DEFAULT now(),
    actor_id text NOT NULL,
    method text NOT NULL,
    route text NOT NULL,
    status integer NOT NULL,
    duration_ms bigint NOT NULL
);

CREATE INDEX ix_system_operation_log_recorded_at
    ON ms_system_operation_log (recorded_at);
