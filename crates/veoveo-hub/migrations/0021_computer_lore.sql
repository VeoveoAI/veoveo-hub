-- A Computer reads and writes Lore as its owner after the owner's one Hub login.
-- Development hard cut: reset Computers instead of migrating them.

-- The Computer's own OAuth client, whose identifier is its native name. The gateway
-- refreshes the sandbox's Lore token with it; each token is the owner's, for every
-- repository the owner is assigned. Each custody handoff gives it a new secret.
ALTER TABLE hub.computer
    ADD COLUMN lore_client_digest text UNIQUE
        CHECK (lore_client_digest ~ '^sha256:[0-9a-f]{64}$');
COMMENT ON COLUMN hub.computer.lore_client_digest IS
    'The SHA-256 digest of the secret of the Computer''s own Lore client, which authenticates the gateway''s refresh of the sandbox''s Lore token; each handoff replaces it.';
