-- Activation owns desired/serving provenance, not native sandbox inventory.
CREATE TABLE hub.activation_build (
    id uuid PRIMARY KEY,
    deployment uuid NOT NULL UNIQUE,
    owner_tenant text NOT NULL CHECK (owner_tenant <> ''),
    owner_principal text NOT NULL CHECK (owner_principal <> ''),
    environment uuid NOT NULL,
    component text NOT NULL CHECK (component <> ''),
    generation bigint NOT NULL CHECK (generation > 0),
    idempotency text NOT NULL CHECK (idempotency <> ''),
    accepted jsonb NOT NULL CHECK (jsonb_typeof(accepted) = 'object'),
    state jsonb NOT NULL CHECK (jsonb_typeof(state) = 'object'),
    created_at timestamptz NOT NULL,
    updated_at timestamptz NOT NULL,
    UNIQUE (owner_tenant, owner_principal, environment, component, generation),
    UNIQUE (owner_tenant, owner_principal, environment, component, idempotency),
    CHECK ((accepted ->> 'build')::uuid = id),
    CHECK ((accepted ->> 'deployment')::uuid = deployment),
    CHECK ((accepted ->> 'generation')::bigint = generation)
);
CREATE TABLE hub.activation_head (
    owner_tenant text NOT NULL,
    owner_principal text NOT NULL,
    environment uuid NOT NULL,
    component text NOT NULL,
    generation bigint NOT NULL CHECK (generation > 0),
    desired uuid NOT NULL REFERENCES hub.activation_build(id),
    serving uuid REFERENCES hub.activation_build(id),
    PRIMARY KEY (owner_tenant, owner_principal, environment, component)
);
CREATE INDEX activation_head_desired ON hub.activation_head(desired);
CREATE INDEX activation_head_serving ON hub.activation_head(serving);
COMMENT ON TABLE hub.activation_build IS 'Retention: source/build/worker provenance is retained while desired, serving, or canonical Task routes/results require it; retirement is explicit and does not replay on another deployment.';
COMMENT ON TABLE hub.activation_head IS 'Retention: durable owned component desired/serving pointers survive Hub and native runtime restart; explicit owner withdrawal changes selection, never implicit fallback on unhealthy override.';

-- Executable source-adapter records, not a second user manifest. Native selectors
-- are retained with the exact accepted generation, never in generic Core/SDK.
CREATE TABLE hub.activation_source (
    build uuid PRIMARY KEY REFERENCES hub.activation_build(id),
    document jsonb NOT NULL CHECK (jsonb_typeof(document) = 'object')
);
COMMENT ON TABLE hub.activation_source IS 'Retention: typed native source provenance and explicit follow selection remain while the associated activation/build/retained worker is required; no bearer or signed URL is stored.';
CREATE TABLE hub.environment_apply (
    owner_tenant text NOT NULL,
    owner_principal text NOT NULL,
    environment uuid NOT NULL,
    idempotency text NOT NULL,
    document jsonb NOT NULL CHECK (jsonb_typeof(document) = 'object'),
    PRIMARY KEY(owner_tenant,owner_principal,environment,idempotency)
);
COMMENT ON TABLE hub.environment_apply IS 'Retention: immutable owner apply idempotency receipts prevent an old definition retry from reapplying or replacing a newer desired definition; development reset removes them.';
