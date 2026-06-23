-- 任务工作量(task point / 故事点):拆分任务的相对工作量估算。0 = 未估。
ALTER TABLE ms_task ADD COLUMN IF NOT EXISTS points INT NOT NULL DEFAULT 0;
