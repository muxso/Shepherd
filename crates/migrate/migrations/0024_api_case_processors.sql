-- 接口用例的前后置处理器(EXTRACT 参数提取 / TIME_WAITING 等待),JSONB 数组。
-- 串行执行时:WAIT 在请求前 sleep,EXTRACT 在请求后把值写入运行上下文供后续步骤 ${var} 使用。
-- 既有行默认空数组,不影响现有批量执行链路。
ALTER TABLE ms_api_case ADD COLUMN IF NOT EXISTS processors JSONB NOT NULL DEFAULT '[]';
