-- Native refresh families remain the browser authority; these rows contain no grants.
-- A pending custody handoff survives a worker rollback/crash and requires a new login.
CREATE TABLE hub.computer (
    id uuid PRIMARY KEY,
    tenant text NOT NULL CHECK (tenant <> ''),
    principal_id text NOT NULL CHECK (principal_id <> ''),
    workspace_request uuid NOT NULL,
    template_request uuid NOT NULL,
    model_provider_request uuid NOT NULL,
    hub_provider_request uuid NOT NULL,
    sandbox_request uuid NOT NULL,
    handoff_id uuid,
    configure_request uuid,
    rotate_request uuid,
    family_id uuid,
    ready boolean NOT NULL DEFAULT false,
    created_at timestamptz NOT NULL,
    UNIQUE (tenant, principal_id),
    CHECK (NOT ready OR (family_id IS NOT NULL AND handoff_id IS NULL)),
    CHECK (handoff_id IS NULL OR (NOT ready AND family_id IS NULL))
);

CREATE TABLE hub.computer_browser_session (
    digest text PRIMARY KEY CHECK (digest ~ '^sha256:[0-9a-f]{64}$'),
    computer_id uuid NOT NULL REFERENCES hub.computer(id) ON DELETE CASCADE,
    family_id uuid NOT NULL REFERENCES hub.refresh_family(id) ON DELETE CASCADE,
    created_at timestamptz NOT NULL
);
CREATE INDEX computer_browser_session_computer ON hub.computer_browser_session(computer_id);
CREATE INDEX computer_browser_session_family ON hub.computer_browser_session(family_id);

-- Single-use PKCE state bound to the browser that initiated login. The verifier is
-- short-lived authorization request material, not an access or refresh grant.
CREATE TABLE hub.computer_login (
    state_digest text PRIMARY KEY CHECK (state_digest ~ '^sha256:[0-9a-f]{64}$'),
    browser_digest text NOT NULL CHECK (browser_digest ~ '^sha256:[0-9a-f]{64}$'),
    verifier text NOT NULL CHECK (length(verifier) BETWEEN 43 AND 128),
    expires_at timestamptz NOT NULL
);
CREATE INDEX computer_login_expiry ON hub.computer_login(expires_at);

COMMENT ON TABLE hub.computer IS
    'Retention: durable owner mapping for the life of the installation, unless '
    'explicitly deleted; browser logout, family expiry and runtime loss do not remove it.';

COMMENT ON TABLE hub.computer_browser_session IS
    'Retention: until its native refresh family or Computer is deleted; foreign-key '
    'cascades remove it. Expired or revoked families cannot authorize retained rows.';

COMMENT ON TABLE hub.computer_login IS
    'Retention: until consumed once by its initiating browser or expired; the Hub '
    'prunes expired login state in batches.';
