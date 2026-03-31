-- Add migration script here
CREATE TYPE user_role as ENUM ('admin', 'chat_user', 'user');

ALTER TABLE users ADD COLUMN role user_role DEFAULT 'admin';