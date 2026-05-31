-- 用户增加启停 / 软删除列(用户管理 CRUD 用)。
ALTER TABLE ms_user ADD COLUMN enable  BOOLEAN NOT NULL DEFAULT true;
ALTER TABLE ms_user ADD COLUMN deleted BOOLEAN NOT NULL DEFAULT false;
