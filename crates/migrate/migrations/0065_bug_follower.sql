-- 缺陷关注人(关注人):谁在关注某条缺陷(多对多)。
-- PK 去重保证幂等关注;缺陷删除时级联清掉其关注关系。user_id 不外键 ms_user(与任务负责人一致,松耦合)。
CREATE TABLE ms_bug_follower (
    bug_id     TEXT NOT NULL REFERENCES ms_bug(id) ON DELETE CASCADE,
    user_id    TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (bug_id, user_id)
);
-- 「我关注的缺陷」反查。
CREATE INDEX ix_bug_follower_user ON ms_bug_follower (user_id);
