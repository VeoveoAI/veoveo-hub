-- Each instance runs in its factory sandbox, which OpenShell starts, restarts and
-- reports on. The Hub records only what OpenShell cannot know: the latest pusher, the
-- last revision the Hub applied, why its own last step failed and the capacity warning
-- of the instance's creation.
ALTER TABLE hub.instance
    ADD COLUMN pusher text CHECK (pusher <> ''),
    ADD COLUMN revision text CHECK (revision ~ '^[0-9a-f]{64}$'),
    ADD COLUMN failed_stage text CHECK (failed_stage <> ''),
    ADD COLUMN failed_message text,
    ADD COLUMN warning text,
    ADD CONSTRAINT instance_failure CHECK ((failed_stage IS NULL) = (failed_message IS NULL));
CREATE INDEX instance_pusher ON hub.instance (pusher);
COMMENT ON COLUMN hub.instance.pusher IS
    'The Hub principal who pushed the revision the Hub last applied or attempted; per-user caps count instances by it.';
COMMENT ON COLUMN hub.instance.revision IS
    'The Lore revision hash signature the Hub last applied to the instance''s sandbox; its start command fetches the branch''s latest revision.';
COMMENT ON COLUMN hub.instance.failed_stage IS
    'The Hub''s step that failed last for the instance, until a later step succeeds.';
COMMENT ON COLUMN hub.instance.warning IS
    'Why the instance was created above a soft cap.';
