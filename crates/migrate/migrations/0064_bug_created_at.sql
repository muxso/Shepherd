-- 缺陷补充创建时间(列表按时间倒序展示用)。
-- 历史行回填为当前时间(无更早的真实时间可考据)。
ALTER TABLE ms_bug
    ADD COLUMN created_at BIGINT NOT NULL DEFAULT (extract(epoch from now()) * 1000)::bigint;

CREATE INDEX ix_bug_project_created ON ms_bug (project_id, created_at DESC) WHERE deleted = false;
