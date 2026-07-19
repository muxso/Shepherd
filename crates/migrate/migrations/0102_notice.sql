-- In-app notifications: one row per receiver; category drives the message-center
-- sidebar (PLAN / BUG / CASE / API / SCHEDULE), resource_type/resource_id drive
-- click-through navigation, at_mention marks "@me" messages.
CREATE TABLE IF NOT EXISTS ms_notice (
    id            uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id    text NOT NULL DEFAULT '',
    receiver_id   text NOT NULL,
    category      text NOT NULL,
    event_type    text NOT NULL,
    title         text NOT NULL,
    content       text NOT NULL DEFAULT '',
    resource_type text NOT NULL DEFAULT '',
    resource_id   text NOT NULL DEFAULT '',
    operator      text NOT NULL DEFAULT '',
    at_mention    boolean NOT NULL DEFAULT false,
    read          boolean NOT NULL DEFAULT false,
    created_at    timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_notice_receiver_read
    ON ms_notice (receiver_id, read);
CREATE INDEX IF NOT EXISTS idx_notice_project_receiver_created
    ON ms_notice (project_id, receiver_id, created_at DESC);
