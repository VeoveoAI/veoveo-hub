-- A factory sandbox reads its repository through Lore as the branch's latest pusher.

-- An instance's own OAuth client. The gateway refreshes the sandbox's Lore token with
-- it; each token is issued for the instance's latest pusher and its repository, under
-- that person's current grant. The client goes with the instance.
ALTER TABLE hub.instance
    ADD COLUMN client_digest text UNIQUE CHECK (client_digest ~ '^sha256:[0-9a-f]{64}$');
COMMENT ON COLUMN hub.instance.client_digest IS
    'The SHA-256 digest of the secret of the instance''s client, which authenticates the gateway''s refresh of the sandbox''s Lore token; a new sandbox gets a new secret.';
