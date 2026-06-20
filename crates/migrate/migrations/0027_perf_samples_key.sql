-- 压测原始样本的存储键(Parquet 对象路径,如 perf/run_id=<id>/part-0.parquet)。
-- 聚合摘要留本表;原始逐请求样本下沉对象存储,这里只存指针,供明细下钻。
ALTER TABLE ms_perf_report ADD COLUMN samples_key text;
