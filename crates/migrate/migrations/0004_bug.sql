-- bug-management:状态项 / 流转边 / 缺陷。
CREATE TABLE ms_status_item (
    id         TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    name       TEXT NOT NULL,
    internal   BOOLEAN NOT NULL DEFAULT false
);
CREATE INDEX ix_status_item_project ON ms_status_item (project_id);

CREATE TABLE ms_status_flow (
    project_id TEXT NOT NULL,
    from_id    TEXT NOT NULL,
    to_id      TEXT NOT NULL,
    PRIMARY KEY (project_id, from_id, to_id)
);

CREATE TABLE ms_bug (
    id         TEXT PRIMARY KEY DEFAULT gen_random_uuid()::text,
    project_id TEXT NOT NULL,
    title      TEXT NOT NULL,
    status     TEXT NOT NULL,
    deleted    BOOLEAN NOT NULL DEFAULT false
);
CREATE INDEX ix_bug_project ON ms_bug (project_id) WHERE deleted = false;
