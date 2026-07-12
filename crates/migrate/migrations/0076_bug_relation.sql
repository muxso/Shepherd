-- 缺陷 ↔ 资产 关联(需求 / 场景用例 / 功能用例)。多对多:
--   一个缺陷可回溯到多条需求/用例;一条需求/用例也可挂多个缺陷。
-- 与 ms_requirement_case(需求↔功能用例)并列,打通「缺陷 → 来源资产」追溯链。
CREATE TABLE IF NOT EXISTS ms_bug_relation (
    bug_id      TEXT NOT NULL,
    kind        TEXT NOT NULL, -- REQUIREMENT | SCENARIO | FUNCTIONAL_CASE
    target_id   TEXT NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (bug_id, kind, target_id)
);
CREATE INDEX IF NOT EXISTS ix_bug_relation_target ON ms_bug_relation (kind, target_id);
