-- 项目成员:谁在项目里 + 角色(OWNER/MEMBER/VIEWER)。
-- 一个用户在一个项目至多一行(主键去重);改角色即 upsert。
CREATE TABLE IF NOT EXISTS ms_project_member (
    project_id text NOT NULL,
    user_id    text NOT NULL,
    role       text NOT NULL DEFAULT 'MEMBER',
    added_at   timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (project_id, user_id)
);

CREATE INDEX IF NOT EXISTS idx_project_member_project
    ON ms_project_member (project_id, added_at);
