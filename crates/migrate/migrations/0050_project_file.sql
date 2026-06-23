-- 0050: 项目文件管理(文件管理标签)。内容以 base64 文本存 PG(小文件;受全局 2MB
-- 请求体上限约束)。后续大文件可迁移到对象存储 + content 仅存指针。
CREATE TABLE IF NOT EXISTS ms_project_file (
    id          text PRIMARY KEY DEFAULT gen_random_uuid()::text,
    project_id  text NOT NULL,
    name        text NOT NULL,
    file_format text NOT NULL DEFAULT '',
    size_bytes  bigint NOT NULL DEFAULT 0,
    content_b64 text NOT NULL DEFAULT '',
    created_by  text,
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS ix_project_file_project ON ms_project_file (project_id, created_at DESC);
