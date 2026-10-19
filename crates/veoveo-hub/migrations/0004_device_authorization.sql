-- Development hard cut to a real typed broker destination: no fabricated code
-- redirect or client PKCE belongs to a device authorization transaction.
DELETE FROM hub.authorization_request;
ALTER TABLE hub.authorization_request DROP COLUMN redirect_uri, DROP COLUMN client_state,
    DROP COLUMN code_challenge, DROP COLUMN code_challenge_method;
ALTER TABLE hub.authorization_request ADD COLUMN destination jsonb NOT NULL CHECK (jsonb_typeof(destination) = 'object');

CREATE TABLE hub.device_authorization (
    id uuid PRIMARY KEY,
    device_digest text NOT NULL UNIQUE CHECK (device_digest ~ '^sha256:[0-9a-f]{64}$'),
    user_digest text NOT NULL UNIQUE CHECK (user_digest ~ '^sha256:[0-9a-f]{64}$'),
    protected_resource text NOT NULL CHECK (protected_resource <> ''),
    oauth_client_id text NOT NULL CHECK (oauth_client_id <> ''),
    work_context text NOT NULL CHECK (work_context <> ''),
    scopes text[] NOT NULL,
    state text NOT NULL CHECK (state IN ('pending','authorizing','approved','denied','consumed')),
    browser_digest text CHECK (browser_digest ~ '^sha256:[0-9a-f]{64}$'),
    principal jsonb CHECK (jsonb_typeof(principal) = 'object'),
    principal_display_name text CHECK (principal_display_name <> ''),
    poll_interval integer NOT NULL CHECK (poll_interval >= 5),
    next_poll timestamptz NOT NULL,
    created_at timestamptz NOT NULL,
    expires_at timestamptz NOT NULL,
    CHECK (created_at < expires_at),
    CHECK ((state IN ('approved','consumed')) = (principal IS NOT NULL AND principal_display_name IS NOT NULL))
);
CREATE INDEX device_authorization_expiry ON hub.device_authorization(expires_at);
COMMENT ON TABLE hub.device_authorization IS
    'Retention: short-lived device authorization, one-time redeemed or denied; expired rows are removed in bounded batches. Only digests of device/user/browser codes are retained.';
