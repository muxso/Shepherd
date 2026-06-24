-- 需求评审不通过:记录最近一次「评审不通过」原因。
-- 需求留在 DRAFT(待修订后重新评审);评审通过(set_baseline)或修订(revise)时清空。
ALTER TABLE ms_requirement ADD COLUMN IF NOT EXISTS review_comment TEXT;
