-- A person's token on the content domain, naming one item of one owner. The token itself is
-- stored only as its digest. It is derived from the nonce and the browser's session cookie,
-- so the same browser recomputes it while it lives and the database alone cannot.
CREATE TABLE hub.content_token (
    digest text PRIMARY KEY CHECK (digest ~ '^sha256:[0-9a-f]{64}$'),
    nonce bytea NOT NULL CHECK (octet_length(nonce) = 16),
    session_digest text NOT NULL CHECK (session_digest ~ '^sha256:[0-9a-f]{64}$'),
    owner text NOT NULL CHECK (owner <> ''),
    item text NOT NULL CHECK (item <> ''),
    computer_id uuid NOT NULL REFERENCES hub.computer(id) ON DELETE CASCADE,
    family_id uuid NOT NULL REFERENCES hub.refresh_family(id) ON DELETE CASCADE,
    expires_at timestamptz NOT NULL,
    hard_expires_at timestamptz NOT NULL,
    CHECK (expires_at <= hard_expires_at)
);
CREATE INDEX content_token_session ON hub.content_token(session_digest, owner, item);
CREATE INDEX content_token_expiry ON hub.content_token(expires_at);
CREATE INDEX content_token_computer ON hub.content_token(computer_id);
CREATE INDEX content_token_family ON hub.content_token(family_id);

COMMENT ON TABLE hub.content_token IS
    'Retention: seven days past its expiry, so a stale tab recovers on reload; the Hub prunes '
    'old rows in batches when it mints another, and foreign-key cascades remove a deleted '
    'family''s or Computer''s rows. An expired row or revoked family authorizes nothing.';
