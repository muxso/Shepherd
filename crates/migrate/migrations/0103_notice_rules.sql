-- Notification routing: webhook robots (Feishu / DingTalk / WeCom) plus
-- per-event rules deciding which channels (IN_APP / ROBOT) an event fans out to.
CREATE TABLE IF NOT EXISTS ms_notice_robot (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id  text NOT NULL,
    name        text NOT NULL,
    platform    text NOT NULL,              -- FEISHU | DINGTALK | WECOM
    webhook_url text NOT NULL,
    secret      text NOT NULL DEFAULT '',   -- DingTalk sign secret (empty = no signing)
    enabled     boolean NOT NULL DEFAULT true,
    created_at  timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_notice_robot_project ON ms_notice_robot (project_id);

CREATE TABLE IF NOT EXISTS ms_notice_rule (
    id         uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id text NOT NULL,
    event_type text NOT NULL,               -- producer event type, or '*' for all
    channels   jsonb NOT NULL DEFAULT '["IN_APP"]',  -- subset of ["IN_APP","ROBOT"]
    robot_ids  jsonb NOT NULL DEFAULT '[]',
    template   text NOT NULL DEFAULT '',    -- ${title} ${operator} ${time}; empty = default text
    enabled    boolean NOT NULL DEFAULT true,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_notice_rule_project_event ON ms_notice_rule (project_id, event_type);
