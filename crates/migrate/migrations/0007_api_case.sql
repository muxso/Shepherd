-- api-test:接口用例定义(供原生 Rust runner 执行)。
-- assertions 以 JSONB 存断言数组(对应 ms-apitest-runner 的 Assertion 序列化形式)。
CREATE TABLE ms_api_case (
    id         TEXT PRIMARY KEY,
    method     TEXT NOT NULL DEFAULT 'GET',
    url        TEXT NOT NULL,
    body       TEXT,
    assertions JSONB NOT NULL DEFAULT '[]'
);
