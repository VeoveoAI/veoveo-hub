-- Immutable canonical routing captures. Store the original bytes: jsonb text is not
-- the SDK canonical encoding and must never be fed into its strict byte importer.
CREATE TABLE hub.execution_composition (
    revision text PRIMARY KEY CHECK (revision ~ '^sha256:[0-9a-f]{64}$'),
    environment uuid NOT NULL,
    canonical bytea NOT NULL,
    created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
COMMENT ON TABLE hub.execution_composition IS
    'Retention: immutable execution captures remain while environments, pins or retained operations can refer to them; explicit operator reset removes development state.';

-- Every deployment has one immutable registration declaration, regardless of how many
-- process registrations it has held. Deleting an OAuth registration cannot free its
-- deployment ID for a different key, namespace, endpoint, contract or storage binding.
CREATE TABLE hub.deployment (
    id uuid PRIMARY KEY,
    server text NOT NULL REFERENCES hub.namespace_claim (server),
    metadata jsonb NOT NULL CHECK (jsonb_typeof(metadata) = 'object'),
    storage_binding jsonb NOT NULL CHECK (jsonb_typeof(storage_binding) = 'object'),
    created_at timestamptz NOT NULL,
    environment uuid,
    owner_tenant text,
    owner_principal text,
    runtime jsonb,
    CHECK ((environment IS NULL AND owner_tenant IS NULL AND owner_principal IS NULL AND runtime IS NULL)
        OR (environment IS NOT NULL AND owner_tenant IS NOT NULL AND owner_principal IS NOT NULL AND runtime IS NOT NULL)),
    CHECK ((metadata ->> 'veoveo_deployment_id')::uuid = id),
    CHECK (metadata ->> 'software_id' = server)
);
COMMENT ON TABLE hub.deployment IS
    'Retention: immutable deployment bindings remain for retained compositions and operations after registration removal; explicit operator reset removes development state.';

-- Authority to register an independently keyed deployment is separate from ownership
-- of a logical namespace. The selected shared storage binding is explicit in this
-- admitted record; environment edit permission alone cannot mint this authority.
CREATE TABLE hub.deployment_admission (
    deployment uuid PRIMARY KEY REFERENCES hub.deployment (id),
    environment uuid NOT NULL,
    owner_tenant text NOT NULL CHECK (owner_tenant <> ''),
    owner_principal text NOT NULL CHECK (owner_principal <> ''),
    server text NOT NULL REFERENCES hub.namespace_claim (server),
    metadata jsonb NOT NULL CHECK (jsonb_typeof(metadata) = 'object'),
    storage_binding jsonb NOT NULL CHECK (jsonb_typeof(storage_binding) = 'object'),
    runtime jsonb NOT NULL CHECK (jsonb_typeof(runtime) = 'object'),
    token_digest text NOT NULL UNIQUE CHECK (token_digest ~ '^sha256:[0-9a-f]{64}$'),
    expires_at timestamptz NOT NULL,
    consumed_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    CHECK ((metadata ->> 'veoveo_deployment_id')::uuid = deployment),
    CHECK (metadata ->> 'software_id' = server)
);
COMMENT ON TABLE hub.deployment_admission IS
    'Retention: one current initial admission remains per immutable deployment until trusted reissue or reset; expired or consumed grants cannot create a new registration.';

-- This hard cut requires reset/reseed of old development registrations.
ALTER TABLE hub.registration ADD COLUMN deployment uuid NOT NULL REFERENCES hub.deployment (id);
CREATE INDEX registration_deployment ON hub.registration (deployment);
CREATE INDEX deployment_server ON hub.deployment (server);
CREATE INDEX deployment_admission_server ON hub.deployment_admission (server);

CREATE TRIGGER deployment_admission_change
    AFTER INSERT OR UPDATE OR DELETE ON hub.deployment_admission
    FOR EACH ROW EXECUTE FUNCTION kernel.notify_change('deployment');
