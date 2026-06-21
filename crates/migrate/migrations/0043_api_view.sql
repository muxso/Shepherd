-- 接口列表「视图」:保存筛选条件 + 列设置 + 页大小(对齐 MeterSphere 视图保存/分享)。
-- config 为不透明 JSON(前端约定:{logic, conds, hiddenCols, pageSize});shared=true 即项目内共享(可被分享链接打开)。
CREATE TABLE ms_api_view (
    id         TEXT PRIMARY KEY DEFAULT gen_random_uuid()::text,
    project_id TEXT NOT NULL,
    user_id    TEXT NOT NULL,
    name       TEXT NOT NULL,
    config     JSONB NOT NULL DEFAULT '{}',
    shared     BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX ix_api_view_project ON ms_api_view (project_id);
