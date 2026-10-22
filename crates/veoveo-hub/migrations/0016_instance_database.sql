-- An instance's server uses the database of its data environment: the installation
-- database for the base branch, and another database of the same PostgreSQL instance
-- for any other branch name. The Hub's own state stays in the installation database.
ALTER TABLE hub.instance
    ADD COLUMN database text NOT NULL CHECK (database ~ '^[a-z0-9_]{1,63}$');
CREATE INDEX instance_database ON hub.instance (database);
COMMENT ON COLUMN hub.instance.database IS
    'The PostgreSQL database the instance''s server registers into; the Hub drops a data environment''s database once no instance names it.';
