-- 接口用例 / Mock 增 MeterSphere 对齐属性列。均可空带默认,既有 runner 读取链路不受影响。

-- 用例:优先级 / 状态 / 标签 / 请求头。
-- case_status 单独命名避免与 ms_api_definition.status 在联表时歧义;仓储以 `case_status AS status` 读出。
ALTER TABLE ms_api_case ADD COLUMN IF NOT EXISTS priority TEXT NOT NULL DEFAULT 'P0';
ALTER TABLE ms_api_case ADD COLUMN IF NOT EXISTS case_status TEXT NOT NULL DEFAULT '进行中';
ALTER TABLE ms_api_case ADD COLUMN IF NOT EXISTS tags JSONB NOT NULL DEFAULT '[]';
ALTER TABLE ms_api_case ADD COLUMN IF NOT EXISTS headers JSONB NOT NULL DEFAULT '[]';

-- Mock:标签 / 自定义响应头 / 响应延时(毫秒)/ 跟随 API 定义。
ALTER TABLE ms_api_mock ADD COLUMN IF NOT EXISTS tags JSONB NOT NULL DEFAULT '[]';
ALTER TABLE ms_api_mock ADD COLUMN IF NOT EXISTS response_headers JSONB NOT NULL DEFAULT '[]';
ALTER TABLE ms_api_mock ADD COLUMN IF NOT EXISTS response_delay_ms INT NOT NULL DEFAULT 0;
ALTER TABLE ms_api_mock ADD COLUMN IF NOT EXISTS follow_definition BOOLEAN NOT NULL DEFAULT false;
