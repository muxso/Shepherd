-- 通用评论:多态挂在任意业务实体 (target_type, target_id) 上。
-- target_type 例:BUG / REQUIREMENT / FUNCTIONAL_CASE / CASE_REVIEW … target_id 为该实体主键。
-- author 存会话用户 id(后端写入,前端不可伪造);软删保留审计痕迹。
CREATE TABLE IF NOT EXISTS ms_comment (
    id          text PRIMARY KEY DEFAULT gen_random_uuid()::text,
    target_type text NOT NULL,
    target_id   text NOT NULL,
    content     text NOT NULL,
    author      text NOT NULL DEFAULT '',
    created_at  timestamptz NOT NULL DEFAULT now(),
    deleted     boolean NOT NULL DEFAULT false
);

CREATE INDEX IF NOT EXISTS idx_comment_target
    ON ms_comment (target_type, target_id, created_at) WHERE deleted = false;
