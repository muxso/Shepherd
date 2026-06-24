-- 设计提案(OpenSpec / BMAD 的 Design 阶段):需求 → 设计稿 → 人审批门 → 修订循环。
-- 插在「拆分」之前;status: DRAFTING / PENDING_REVIEW / APPROVED / CHANGES_REQUESTED。
CREATE TABLE ms_design_proposal (
    seq             BIGSERIAL,
    id              TEXT PRIMARY KEY DEFAULT gen_random_uuid()::text,
    requirement_id  TEXT NOT NULL,
    title           TEXT NOT NULL,
    design_doc      TEXT,                          -- 当前设计稿(markdown);未提交为 NULL
    status          TEXT NOT NULL DEFAULT 'DRAFTING',
    review_comment  TEXT,                          -- 最近一次驳回的评审意见(反馈 agent 修订)
    revision        INT  NOT NULL DEFAULT 0        -- 修订轮次:每驳回 +1
);
CREATE INDEX ix_ms_design_proposal_req ON ms_design_proposal (requirement_id, seq);
