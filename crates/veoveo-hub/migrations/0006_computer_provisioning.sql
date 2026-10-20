-- Development hard cut: apply to a reset/reseeded application database.
-- No backfill: an existing Computer must never silently inherit new defaults.
ALTER TABLE hub.computer ADD COLUMN provisioning jsonb NOT NULL
    CHECK (jsonb_typeof(provisioning) = 'object');
COMMENT ON COLUMN hub.computer.provisioning IS
    'Immutable admitted per-instance native configuration, policy/template protobufs and secret locators; no credential material. Retained with the owner mapping until explicit deletion.';

ALTER TABLE hub.computer_login ADD COLUMN provisioning jsonb NOT NULL
    CHECK (jsonb_typeof(provisioning) = 'object');
COMMENT ON COLUMN hub.computer_login.provisioning IS
    'Initiating admitted client/context/provision snapshot; consumed with PKCE state or deleted by expiry GC. Secret locators only, no credential values.';
