-- Public share links for reports (scenario batch reports and test-plan reports).
-- A share row maps an unguessable token to one report; the public read endpoints
-- resolve the token without authentication so a link can be opened by anyone.
CREATE TABLE IF NOT EXISTS ms_report_share (
    token        TEXT PRIMARY KEY,
    report_type  TEXT NOT NULL,   -- 'scenario' | 'plan'
    report_id    TEXT NOT NULL,
    project_id   TEXT,
    created_by   TEXT,
    created_at   BIGINT NOT NULL,
    expires_at   BIGINT,          -- NULL = never expires
    revoked      BOOLEAN NOT NULL DEFAULT FALSE
);
CREATE INDEX IF NOT EXISTS idx_report_share_target ON ms_report_share (report_type, report_id);
