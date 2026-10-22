-- An instance is one branch of a server repository, served from its own sandbox. Its
-- registration token authorizes every registration of its deployment, and each
-- registration replaces the previous one. The deployment row appears at the first
-- registration, so this table does not reference it.
CREATE TABLE hub.instance (
    deployment uuid PRIMARY KEY,
    token_digest text NOT NULL UNIQUE CHECK (token_digest ~ '^sha256:[0-9a-f]{64}$'),
    created_at timestamptz NOT NULL
);
COMMENT ON TABLE hub.instance IS
    'Retention: one row per instance while it exists; removing the instance removes it, and explicit operator reset removes development state.';

-- An instance registration claims no namespace; the namespace path still writes its
-- claim before its rows.
ALTER TABLE hub.registration DROP CONSTRAINT registration_server_fkey;
ALTER TABLE hub.deployment DROP CONSTRAINT deployment_server_fkey;

COMMENT ON TABLE hub.deployment IS
    'Retention: deployment bindings remain for retained compositions and operations after registration removal; an instance''s registration replaces its binding, and explicit operator reset removes development state.';

COMMENT ON TABLE hub.registration IS
    'Retention: until the server deregisters or a new registration of its instance replaces it; outside an instance, also until its login has held no live kernel instance for the grace period. The Hub removes it with its login.';
