-- 用户增加「系统管理员」标记列(系统管理 / Administrator)。
ALTER TABLE ms_user ADD COLUMN admin BOOLEAN NOT NULL DEFAULT false;
