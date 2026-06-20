-- agent 协议能力快照:注册时从 agent 的 /protocols 拉取并落库,
-- 中央据此「按协议选 agent」(POST /runner/probe 时筛出支持该协议的 agent 路由)。
-- 既有行默认 {http}(原 runner-agent 至少支持 http)。
ALTER TABLE ms_runner_agent
    ADD COLUMN protocols text[] NOT NULL DEFAULT '{http}';
