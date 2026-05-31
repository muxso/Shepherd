-- case-management:评审配置 / 历史 / 用例状态。
CREATE TABLE ms_case_review (
    id             TEXT PRIMARY KEY,
    pass_rule      TEXT NOT NULL,   -- SINGLE | MULTIPLE
    reviewer_count INT  NOT NULL
);

CREATE TABLE ms_case_review_history (
    seq         BIGSERIAL PRIMARY KEY,
    review_id   TEXT NOT NULL,
    case_id     TEXT NOT NULL,
    reviewer_id TEXT NOT NULL,
    status      TEXT NOT NULL
);
CREATE INDEX ix_case_review_history ON ms_case_review_history (review_id, case_id, seq);

CREATE TABLE ms_case_review_status (
    review_id TEXT NOT NULL,
    case_id   TEXT NOT NULL,
    status    TEXT NOT NULL,
    PRIMARY KEY (review_id, case_id)
);
