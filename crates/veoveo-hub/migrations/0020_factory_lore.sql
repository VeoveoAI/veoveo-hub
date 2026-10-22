-- A factory sandbox reads its repository through Lore as the branch's latest pusher,
-- and the Hub watches a repository as the person who turned it on.

-- An instance's own OAuth client. The gateway refreshes the sandbox's Lore token with
-- it; each token is issued for the instance's latest pusher and its repository, under
-- that person's current grant. The client goes with the instance.
ALTER TABLE hub.instance
    ADD COLUMN client_digest text UNIQUE CHECK (client_digest ~ '^sha256:[0-9a-f]{64}$');
COMMENT ON COLUMN hub.instance.client_digest IS
    'The SHA-256 digest of the secret of the instance''s client, which authenticates the gateway''s refresh of the sandbox''s Lore token; a new sandbox gets a new secret.';

-- Executable-owned native Lore contribution: the repositories the Hub watches, each as
-- the person who last turned it on.
CREATE TABLE hub.lore_watch (
    deployment text NOT NULL CHECK (deployment <> ''),
    repository text NOT NULL CHECK (repository ~ '^[0-9a-f]{32}$'),
    watcher text NOT NULL CHECK (watcher <> ''),
    -- Why the last subscription as the watcher failed, until one succeeds.
    failed_message text,
    updated_at timestamptz NOT NULL,
    PRIMARY KEY (deployment, repository)
);
COMMENT ON TABLE hub.lore_watch IS
    'Retention: one row per watched repository until explicit operator reset removes development state; a later watch replaces its watcher.';

-- Why the Hub could not prepare the latest push of a watched branch: reading its
-- `openshell/` as the pusher failed. Applying a prepared push records its own failures on
-- the instance; a branch whose first push failed here has no instance yet.
CREATE TABLE hub.lore_prepare_failure (
    deployment text NOT NULL CHECK (deployment <> ''),
    repository text NOT NULL CHECK (repository ~ '^[0-9a-f]{32}$'),
    branch text NOT NULL CHECK (octet_length(branch) BETWEEN 1 AND 1000),
    revision text NOT NULL CHECK (revision ~ '^[0-9a-f]{64}$'),
    pusher text NOT NULL CHECK (pusher <> ''),
    message text NOT NULL,
    failed_at timestamptz NOT NULL,
    PRIMARY KEY (deployment, repository, branch)
);
COMMENT ON TABLE hub.lore_prepare_failure IS
    'Retention: until the branch''s next push is prepared; explicit operator reset removes development state.';
