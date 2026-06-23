-- 场景元信息:描述/标签/等级/所属模块 + 场景参数(常规/CSV)。
-- meta 为不透明 JSON(前端约定:{description, tags, priority, moduleId, params, csvParams});
-- 名称/状态仍为一等列,meta 承载其余可编辑项,便于一次 PATCH 保存(对齐 MeterSphere 场景基本信息/参数)。
ALTER TABLE ms_api_scenario ADD COLUMN IF NOT EXISTS meta JSONB NOT NULL DEFAULT '{}';
