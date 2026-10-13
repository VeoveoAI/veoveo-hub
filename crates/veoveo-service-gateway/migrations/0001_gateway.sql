-- The Gateway's schema: control-plane revisions, hosted OAuth state, server
-- registrations and Task routes. The Gateway connects as the installation owner, applies
-- these migrations after core's kernel migrations and is the only writer; registered
-- servers receive no grant on this schema. sqlx creates the schema for its history table.

ALTER DEFAULT PRIVILEGES IN SCHEMA gateway REVOKE EXECUTE ON FUNCTIONS FROM PUBLIC;

-- Rejects every change to a row of an append-only table.
CREATE FUNCTION gateway.reject_change() RETURNS trigger
LANGUAGE plpgsql
SET search_path = pg_catalog, pg_temp
AS $$
BEGIN
    RAISE EXCEPTION '%.% rows are immutable', TG_TABLE_SCHEMA, TG_TABLE_NAME
        USING ERRCODE = 'restrict_violation';
END
$$;

-- One published installation configuration. A revision never changes once written.
CREATE TABLE gateway.control_revision (
    id text PRIMARY KEY CHECK (id <> ''),
    digest text NOT NULL CHECK (digest ~ '^sha256:[0-9a-f]{64}$'),
    source text NOT NULL CHECK (source IN ('admin_api', 'seed_file')),
    applied_at timestamptz NOT NULL,
    applied_by text NOT NULL CHECK (applied_by <> ''),
    tenant text CHECK (tenant <> ''),
    control_plane jsonb NOT NULL CHECK (jsonb_typeof(control_plane) = 'object')
);

CREATE TRIGGER control_revision_immutable
    BEFORE UPDATE ON gateway.control_revision
    FOR EACH ROW EXECUTE FUNCTION gateway.reject_change();

-- The objects of one revision, one row per configuration object. A profile row names
-- the policy version it evaluates under.
CREATE TABLE gateway.control_object (
    revision text NOT NULL REFERENCES gateway.control_revision (id),
    kind text NOT NULL CHECK (kind ~ '^[A-Za-z0-9_.:/-]{1,128}$'),
    object_id text NOT NULL CHECK (object_id <> ''),
    tenant text CHECK (tenant <> ''),
    document jsonb NOT NULL CHECK (jsonb_typeof(document) = 'object'),
    profile_policy_version text CHECK (
        profile_policy_version <> '' AND profile_policy_version !~ '[[:space:][:cntrl:]]'
    ),
    PRIMARY KEY (revision, kind, object_id),
    CHECK ((kind = 'profile') = (profile_policy_version IS NOT NULL))
);

CREATE TRIGGER control_object_immutable
    BEFORE UPDATE ON gateway.control_object
    FOR EACH ROW EXECUTE FUNCTION gateway.reject_change();

-- The pointer to the active revision. The table holds exactly one row; its revision is
-- null until the first publication. A publication locks the row, so publications
-- serialize across replicas.
CREATE TABLE gateway.control_active (
    singleton boolean PRIMARY KEY DEFAULT true CHECK (singleton),
    revision text REFERENCES gateway.control_revision (id),
    updated_at timestamptz NOT NULL DEFAULT now()
);

INSERT INTO gateway.control_active (singleton) VALUES (true);

CREATE TRIGGER control_active_singleton
    BEFORE INSERT OR DELETE ON gateway.control_active
    FOR EACH STATEMENT EXECUTE FUNCTION gateway.reject_change();

-- A revoked JWT, kept until the token itself expires.
CREATE TABLE gateway.jwt_revocation (
    profile text NOT NULL CHECK (profile <> ''),
    issuer text NOT NULL CHECK (issuer <> ''),
    jwt_id text NOT NULL CHECK (jwt_id <> ''),
    revoked_at timestamptz NOT NULL,
    expires_at timestamptz NOT NULL,
    reason text,
    PRIMARY KEY (profile, issuer, jwt_id)
);

CREATE INDEX jwt_revocation_expiry ON gateway.jwt_revocation (expires_at);

-- The `jti` of a client assertion or ID-JAG seen before it expires. A live row refuses
-- the same identifier; an expired row may be claimed again.
CREATE TABLE gateway.replay_id (
    kind text NOT NULL CHECK (kind IN ('client_assertion', 'id_jag')),
    authorization_server text NOT NULL CHECK (authorization_server <> ''),
    client_id text NOT NULL CHECK (client_id <> ''),
    jwt_id text NOT NULL CHECK (jwt_id <> ''),
    seen_at timestamptz NOT NULL,
    expires_at timestamptz NOT NULL,
    PRIMARY KEY (kind, authorization_server, client_id, jwt_id)
);

CREATE INDEX replay_id_expiry ON gateway.replay_id (kind, expires_at);

-- A hosted authorization request waiting for the identity provider's callback. The
-- callback consumes it by deleting it.
CREATE TABLE gateway.authorization_request (
    idp_state text PRIMARY KEY CHECK (idp_state <> ''),
    profile text NOT NULL CHECK (profile <> ''),
    oauth_client_id text NOT NULL CHECK (oauth_client_id <> ''),
    work_context text NOT NULL CHECK (work_context <> ''),
    oidc_client text NOT NULL CHECK (oidc_client <> ''),
    redirect_uri text NOT NULL CHECK (redirect_uri <> ''),
    client_state text,
    requested_scopes text[] NOT NULL,
    code_challenge text NOT NULL,
    code_challenge_method text NOT NULL CHECK (code_challenge_method = 'S256'),
    idp_code_verifier text NOT NULL,
    idp_code_challenge text NOT NULL,
    idp_code_challenge_method text NOT NULL CHECK (idp_code_challenge_method = 'S256'),
    nonce text NOT NULL CHECK (nonce <> ''),
    created_at timestamptz NOT NULL,
    expires_at timestamptz NOT NULL,
    CHECK (created_at < expires_at)
);

CREATE INDEX authorization_request_expiry ON gateway.authorization_request (expires_at);

-- An issued authorization code and the principal it was issued to. Redemption sets
-- consumed_at once.
CREATE TABLE gateway.authorization_code (
    code text PRIMARY KEY CHECK (code <> ''),
    profile text NOT NULL CHECK (profile <> ''),
    oauth_client_id text NOT NULL CHECK (oauth_client_id <> ''),
    work_context text NOT NULL CHECK (work_context <> ''),
    oidc_client text NOT NULL CHECK (oidc_client <> ''),
    redirect_uri text NOT NULL CHECK (redirect_uri <> ''),
    client_state text,
    scopes text[] NOT NULL,
    code_challenge text NOT NULL,
    code_challenge_method text NOT NULL CHECK (code_challenge_method = 'S256'),
    principal jsonb NOT NULL CHECK (jsonb_typeof(principal) = 'object'),
    principal_display_name text NOT NULL CHECK (principal_display_name <> ''),
    issued_at timestamptz NOT NULL,
    expires_at timestamptz NOT NULL,
    consumed_at timestamptz,
    CHECK (issued_at < expires_at)
);

CREATE INDEX authorization_code_expiry ON gateway.authorization_code (expires_at);

-- A rotating refresh-token family. Access tokens bound to the family stay valid while
-- it is unrevoked and unexpired.
CREATE TABLE gateway.refresh_family (
    id uuid PRIMARY KEY,
    authorization_server text NOT NULL CHECK (authorization_server <> ''),
    profile text NOT NULL CHECK (profile <> ''),
    oauth_client_id text NOT NULL CHECK (oauth_client_id <> ''),
    work_context text NOT NULL CHECK (work_context <> ''),
    principal_id text NOT NULL CHECK (principal_id <> ''),
    tenant text CHECK (tenant <> ''),
    scopes text[] NOT NULL,
    principal jsonb NOT NULL CHECK (jsonb_typeof(principal) = 'object'),
    principal_display_name text NOT NULL CHECK (principal_display_name <> ''),
    current_generation bigint NOT NULL CHECK (current_generation >= 0),
    issued_at timestamptz NOT NULL,
    expires_at timestamptz NOT NULL,
    revoked_at timestamptz,
    revocation_reason text CHECK (revocation_reason IN ('client_revocation', 'token_replay')),
    CHECK (issued_at < expires_at),
    CHECK ((revoked_at IS NULL) = (revocation_reason IS NULL))
);

CREATE INDEX refresh_family_expiry ON gateway.refresh_family (expires_at);

-- One generation of a family. The token itself is never stored, only its digest. A
-- successor carries an encrypted delivery envelope for a short window, so a concurrent
-- presentation of its predecessor receives the same successor.
CREATE TABLE gateway.refresh_token (
    id uuid PRIMARY KEY,
    family uuid NOT NULL REFERENCES gateway.refresh_family (id) ON DELETE CASCADE,
    token_digest text NOT NULL UNIQUE CHECK (token_digest ~ '^sha256:[0-9a-f]{64}$'),
    generation bigint NOT NULL CHECK (generation >= 0),
    issued_at timestamptz NOT NULL,
    expires_at timestamptz NOT NULL,
    consumed_at timestamptz,
    replacement uuid REFERENCES gateway.refresh_token (id) ON DELETE SET NULL,
    replay_detected_at timestamptz,
    delivery_envelope text CHECK (delivery_envelope <> ''),
    delivery_expires_at timestamptz,
    UNIQUE (family, generation),
    CHECK (issued_at < expires_at),
    CHECK ((delivery_envelope IS NULL) = (delivery_expires_at IS NULL)),
    CHECK (replacement IS NULL OR consumed_at IS NOT NULL)
);

CREATE INDEX refresh_token_expiry ON gateway.refresh_token (expires_at);
CREATE INDEX refresh_token_delivery_expiry ON gateway.refresh_token (delivery_expires_at)
    WHERE delivery_expires_at IS NOT NULL;

-- The first registration of a server pins its namespace to the registering key's RFC
-- 7638 thumbprint. Releasing a namespace is an administrative action.
CREATE TABLE gateway.namespace_claim (
    server text PRIMARY KEY CHECK (server ~ '^[a-z][a-z0-9-]{0,47}$'),
    key_thumbprint text NOT NULL CHECK (key_thumbprint ~ '^[A-Za-z0-9_-]{43}$'),
    claimed_at timestamptz NOT NULL
);

-- One RFC 7591 registration and the PostgreSQL login it received. orphaned_at records
-- when the Gateway first found the login without a live kernel instance; it is NULL
-- while the login holds one. The Gateway removes the registration with its login once
-- orphaned_at is older than the installation's grace period, so an installation whose
-- servers stay alive writes nothing while it is idle.
CREATE TABLE gateway.registration (
    client_id text PRIMARY KEY CHECK (client_id <> ''),
    server text NOT NULL REFERENCES gateway.namespace_claim (server),
    metadata jsonb NOT NULL CHECK (jsonb_typeof(metadata) = 'object'),
    software_statement text NOT NULL CHECK (software_statement <> ''),
    access_token_digest text NOT NULL CHECK (access_token_digest ~ '^sha256:[0-9a-f]{64}$'),
    login text NOT NULL UNIQUE CHECK (login ~ '^[a-z_][a-z0-9_]{0,62}$'),
    issued_at timestamptz NOT NULL,
    updated_at timestamptz NOT NULL,
    orphaned_at timestamptz,
    CHECK (metadata ->> 'software_id' = server)
);

CREATE INDEX registration_server ON gateway.registration (server);

-- Replicas follow registrations on the `gateway` channel. Setting or clearing
-- orphaned_at wakes no listener.
CREATE TRIGGER registration_change
    AFTER INSERT OR DELETE OR UPDATE OF metadata, software_statement ON gateway.registration
    FOR EACH ROW EXECUTE FUNCTION kernel.notify_change('client_id');

-- The public identifier of an upstream Task the Gateway projected. The source tuple
-- (server, upstream Task, owner, profile) is unique, so a retried projection finds the
-- original route with its original authority and expiry. The owner columns record the
-- actor and provenance that created the Task; later access must present the same ones
-- and retain the recorded data labels.
CREATE TABLE gateway.task_route (
    id text PRIMARY KEY CHECK (id ~ '^gtr_[A-Za-z0-9_-]{43}$'),
    tenant uuid NOT NULL REFERENCES identity.tenant (id),
    owner uuid NOT NULL REFERENCES identity.principal (id),
    work_context uuid NOT NULL REFERENCES identity.work_context (id) ON DELETE CASCADE,
    profile text NOT NULL CHECK (profile <> ''),
    server text NOT NULL CHECK (server <> ''),
    source_task_id text NOT NULL CHECK (octet_length(source_task_id) BETWEEN 1 AND 512),
    authority_digest text NOT NULL CHECK (authority_digest ~ '^sha256:[0-9a-f]{64}$'),
    owner_kind text NOT NULL CHECK (owner_kind IN ('user', 'service')),
    owner_issuer text NOT NULL CHECK (owner_issuer <> ''),
    owner_subject text NOT NULL CHECK (owner_subject <> ''),
    invocation_mode text NOT NULL CHECK (invocation_mode IN ('direct', 'delegated', 'automated')),
    initiator text CHECK (initiator <> ''),
    delegation_id text CHECK (delegation_id <> ''),
    data_labels text[] NOT NULL,
    created_at timestamptz NOT NULL,
    expires_at timestamptz NOT NULL,
    UNIQUE (server, source_task_id, owner, profile),
    CHECK (created_at < expires_at),
    CHECK ((invocation_mode = 'automated') = (initiator IS NULL)),
    CHECK ((invocation_mode = 'delegated') = (delegation_id IS NOT NULL))
);

-- Deleting a refresh token clears the replacement that names it. This index finds that
-- predecessor without a scan of every token.
CREATE INDEX refresh_token_replacement ON gateway.refresh_token (replacement)
    WHERE replacement IS NOT NULL;

-- Cleanup deletes expired Task routes in expiry order, and removing a Work Context
-- removes its routes through the second index rather than a scan.
CREATE INDEX task_route_expiry ON gateway.task_route (expires_at);
CREATE INDEX task_route_work_context ON gateway.task_route (work_context);

-- Deleting a principal or a tenant checks the Task routes that reference it. These
-- indexes find those routes without a scan of every route.
CREATE INDEX task_route_owner ON gateway.task_route (owner);
CREATE INDEX task_route_tenant ON gateway.task_route (tenant);

-- Every table states how long its rows stay and what removes them.
COMMENT ON TABLE gateway.control_revision IS
    'Retention: for the life of the installation; a revision never changes.';

COMMENT ON TABLE gateway.control_object IS
    'Retention: as long as its revision.';

COMMENT ON TABLE gateway.control_active IS
    'Retention: one row for the life of the installation.';

COMMENT ON TABLE gateway.jwt_revocation IS
    'Retention: until the revoked JWT expires; the Gateway prunes expired revocations '
    'in batches.';

COMMENT ON TABLE gateway.replay_id IS
    'Retention: until the identifier expires; the Gateway prunes expired identifiers in '
    'batches, and a new claim may take an expired row.';

COMMENT ON TABLE gateway.authorization_request IS
    'Retention: until the identity provider''s callback consumes it or it expires; the '
    'Gateway prunes expired requests in batches.';

COMMENT ON TABLE gateway.authorization_code IS
    'Retention: until it expires, redeemed or not; the Gateway prunes expired codes in '
    'batches.';

COMMENT ON TABLE gateway.refresh_family IS
    'Retention: until it expires; the Gateway prunes expired families in batches, and '
    'their tokens go with them.';

COMMENT ON TABLE gateway.refresh_token IS
    'Retention: until it or its family expires; the Gateway prunes expired tokens in '
    'batches and clears a delivery envelope when its window ends.';

COMMENT ON TABLE gateway.namespace_claim IS
    'Retention: until an administrator releases the namespace.';

COMMENT ON TABLE gateway.registration IS
    'Retention: until the server deregisters, or until its login has held no live '
    'kernel instance for the grace period; the Gateway then removes it with its login.';

COMMENT ON TABLE gateway.task_route IS
    'Retention: until it expires or a publication deletes its Work Context; the Gateway '
    'prunes expired routes in batches.';
