-- Executable-owned native Lore contribution. No Computer FK: replacing a
-- disposable Computer must not replace the user's repository association.
CREATE TABLE hub.lore_workspace (
    tenant text NOT NULL CHECK (tenant <> ''),
    principal_id text NOT NULL CHECK (principal_id <> ''),
    deployment text NOT NULL CHECK (deployment <> ''),
    repository text NOT NULL CHECK (repository ~ '^[0-9a-f]{32}$'),
    default_branch text CHECK (default_branch ~ '^[0-9a-f]{32}$'),
    -- Reserved publication identity may precede the immutable revision; no FK.
    publication_id text NOT NULL CHECK (publication_id <> ''),
    -- Last checkout observation only; no cascading ownership through a Computer.
    computer_id uuid,
    last_status text NOT NULL DEFAULT 'preparing' CHECK (last_status IN ('preparing', 'authentication_required', 'denied', 'unavailable', 'conflict', 'ready')),
    name text NOT NULL CHECK (name <> ''),
    mode text NOT NULL CHECK (mode IN ('create', 'reuse')),
    state text NOT NULL DEFAULT 'reserved' CHECK (state IN ('reserved', 'verified', 'ready')),
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    PRIMARY KEY (tenant, principal_id),
    CHECK (mode <> 'create' OR default_branch IS NOT NULL),
    CHECK (state = 'reserved' OR default_branch IS NOT NULL)
);
COMMENT ON TABLE hub.lore_workspace IS
    'Retention: durable native personal workspace binding for the installation; deleted only by an explicit owner reset. Reserved IDs survive interrupted native Create/control publication; no tokens. Retained independently of Computer/session lifecycle. Reuse never grants Owner.';

ALTER TABLE hub.control_revision DROP CONSTRAINT control_revision_source_check;
ALTER TABLE hub.control_revision ADD CHECK (source IN ('admin_api', 'seed_file', 'contribution'));
