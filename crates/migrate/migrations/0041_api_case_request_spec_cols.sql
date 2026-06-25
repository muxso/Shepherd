-- 接口用例补请求规格列(Query / REST 路径参数 / 认证)。
-- 均可空带默认,既有 runner 读取链路(method/url/body/assertions/processors)不受影响。
ALTER TABLE ms_api_case ADD COLUMN IF NOT EXISTS query_params JSONB NOT NULL DEFAULT '[]';
ALTER TABLE ms_api_case ADD COLUMN IF NOT EXISTS rest_params JSONB NOT NULL DEFAULT '[]';
ALTER TABLE ms_api_case ADD COLUMN IF NOT EXISTS auth JSONB NOT NULL DEFAULT '{}';
