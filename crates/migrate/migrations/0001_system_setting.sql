-- system-setting:用户表。
CREATE TABLE ms_user (
    id    TEXT PRIMARY KEY DEFAULT gen_random_uuid()::text,
    name  TEXT NOT NULL,
    email TEXT NOT NULL UNIQUE
);
