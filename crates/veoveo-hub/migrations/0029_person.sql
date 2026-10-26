-- The people who signed in, by tenant, as their identity provider last named them, so that
-- a person can be named by email. Every sign-in writes its person's row.
CREATE TABLE hub.person (
    tenant text NOT NULL CHECK (tenant <> ''),
    principal text NOT NULL CHECK (principal <> ''),
    display_name text NOT NULL CHECK (display_name <> ''),
    -- The provider's email, in lowercase; none when it names no address.
    email text CHECK (
        octet_length(email) <= 254 AND email ~ '^[^@[:space:][:cntrl:]]+@[^@[:space:][:cntrl:]]+$'
    ),
    updated_at timestamptz NOT NULL,
    PRIMARY KEY (tenant, principal),
    -- An address belongs to one person of a tenant; a sign-in with another person's
    -- address takes it over.
    UNIQUE (tenant, email)
);
COMMENT ON TABLE hub.person IS
    'Retention: one row per person of a tenant who signed in, replaced at each sign-in; explicit operator reset removes development state.';
