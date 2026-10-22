-- Development hard cut: an instance is one branch of one Lore repository, and a push
-- reaches it by both. Reset instances instead of migrating them.
ALTER TABLE hub.instance
    ADD COLUMN repository text NOT NULL CHECK (repository ~ '^[0-9a-f]{32}$'),
    ADD COLUMN served_at timestamptz,
    ADD CONSTRAINT instance_repository_branch UNIQUE (repository, branch);
COMMENT ON COLUMN hub.instance.repository IS
    'The Lore repository ID whose branch the instance serves; access to the instance follows a principal''s grant on it.';
COMMENT ON COLUMN hub.instance.served_at IS
    'When the instance''s server last registered or resumed its registration. A push clears it before the Hub restarts or creates the sandbox, so a server that set it since serves the revision the Hub applied.';
