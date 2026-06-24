-- 定时导入计划新增「操作人」:记录最近一次运行是谁触发的。
-- 手动「立即执行」记当前用户;cron 自动运行记空串(前端展示为「自动」)。
ALTER TABLE ms_api_import_schedule
    ADD COLUMN IF NOT EXISTS last_run_by text NOT NULL DEFAULT '';
