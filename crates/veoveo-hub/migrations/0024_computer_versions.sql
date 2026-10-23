-- A Computer has a sandbox at some version, or none. One operation replaces it: delete
-- the sandbox keeping its home, create one of a version, wait until it is ready and
-- record that version as `provisioning`. Each operation derives its native request IDs,
-- so the per-Computer template and sandbox request IDs go. Development hard cut: reset
-- Computers instead of migrating them.
ALTER TABLE hub.computer
    DROP COLUMN template_request,
    DROP COLUMN sandbox_request,
    ADD COLUMN operation_id uuid,
    ADD COLUMN operation_started_at timestamptz,
    ADD COLUMN operation_provisioning jsonb
        CHECK (jsonb_typeof(operation_provisioning) = 'object'),
    ADD COLUMN operation_notice_shown boolean NOT NULL DEFAULT false,
    ADD COLUMN failure jsonb CHECK (jsonb_typeof(failure) = 'object'),
    ADD COLUMN update_notice jsonb CHECK (jsonb_typeof(update_notice) = 'object'),
    ADD CONSTRAINT computer_operation CHECK (
        (operation_id IS NULL) = (operation_started_at IS NULL)
        AND (operation_id IS NULL) = (operation_provisioning IS NULL)
    );
COMMENT ON COLUMN hub.computer.operation_id IS
    'The replacement of the Computer''s sandbox that runs now, with its start and the provision snapshot it creates; a crashed worker''s operation is claimed again once stale. Cleared when it ends.';
COMMENT ON COLUMN hub.computer.operation_notice_shown IS
    'Whether the start page showed the running operation''s changes, so its completion records no update notice.';
COMMENT ON COLUMN hub.computer.failure IS
    'The cause, time and version of the last failed replacement, until the next one begins; the Computer keeps its recorded provisioning.';
COMMENT ON COLUMN hub.computer.update_notice IS
    'The last completed update from one version to another and its changes, until a status read or the start page has shown it.';
