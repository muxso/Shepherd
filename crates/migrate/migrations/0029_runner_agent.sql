-- 远程执行 agent 注册表:按环境登记 runner-agent(部署在目标环境内)。
-- 中央据此把自包含用例派给选定 agent 就地执行。token 为可选共享密钥(派发时带 Bearer)。
CREATE TABLE ms_runner_agent (
    id        text PRIMARY KEY DEFAULT gen_random_uuid()::text,
    name      text NOT NULL,
    base_url  text NOT NULL,
    token     text,
    enabled   boolean NOT NULL DEFAULT true,
    deleted   boolean NOT NULL DEFAULT false
);
