-- A person's sign-in on a server's app origin. The issuer hands the browser a single-use
-- code bound to the state the app origin set in that browser; redeeming the code on the
-- app origin opens a session there of the person's Computer refresh family.
CREATE TABLE hub.app_origin_code (
    code_digest text PRIMARY KEY CHECK (code_digest ~ '^sha256:[0-9a-f]{64}$'),
    state_digest text NOT NULL CHECK (state_digest ~ '^sha256:[0-9a-f]{64}$'),
    server text NOT NULL CHECK (server <> ''),
    computer_id uuid NOT NULL REFERENCES hub.computer(id) ON DELETE CASCADE,
    family_id uuid NOT NULL REFERENCES hub.refresh_family(id) ON DELETE CASCADE,
    return_path text NOT NULL CHECK (return_path LIKE '/%'),
    expires_at timestamptz NOT NULL
);
CREATE INDEX app_origin_code_expiry ON hub.app_origin_code(expires_at);
CREATE INDEX app_origin_code_computer ON hub.app_origin_code(computer_id);
CREATE INDEX app_origin_code_family ON hub.app_origin_code(family_id);

CREATE TABLE hub.app_origin_session (
    digest text PRIMARY KEY CHECK (digest ~ '^sha256:[0-9a-f]{64}$'),
    server text NOT NULL CHECK (server <> ''),
    computer_id uuid NOT NULL REFERENCES hub.computer(id) ON DELETE CASCADE,
    family_id uuid NOT NULL REFERENCES hub.refresh_family(id) ON DELETE CASCADE,
    created_at timestamptz NOT NULL
);
CREATE INDEX app_origin_session_computer ON hub.app_origin_session(computer_id);
CREATE INDEX app_origin_session_family ON hub.app_origin_session(family_id);

COMMENT ON TABLE hub.app_origin_code IS
    'Retention: until consumed once by its initiating browser or expired; the Hub prunes '
    'expired codes in batches when it hands out another.';
COMMENT ON TABLE hub.app_origin_session IS
    'Retention: until its native refresh family or Computer is deleted; foreign-key '
    'cascades remove it. Expired or revoked families cannot authorize retained rows.';
