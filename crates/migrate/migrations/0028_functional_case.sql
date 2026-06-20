-- 功能用例:用例 CRUD + 自定义字段(jsonb)。custom_fields 适配各团队模板差异。
CREATE TABLE ms_functional_case (
    id            text PRIMARY KEY DEFAULT gen_random_uuid()::text,
    project_id    text NOT NULL,
    name          text NOT NULL,
    module        text NOT NULL DEFAULT '',
    priority      text NOT NULL DEFAULT 'P2',
    status        text NOT NULL DEFAULT 'PREPARED',
    custom_fields jsonb NOT NULL DEFAULT '{}'::jsonb,
    deleted       boolean NOT NULL DEFAULT false
);
CREATE INDEX ix_functional_case_project ON ms_functional_case (project_id) WHERE NOT deleted;
