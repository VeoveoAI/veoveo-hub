-- Routing prefers a deployment whose process lives over a newer one whose process
-- ended, so every replica follows the orphan mark too: setting or clearing orphaned_at
-- now notifies the `hub` channel like the other registration changes. Housekeeping
-- writes the mark only when it changes, so an idle installation still writes nothing.
DROP TRIGGER registration_change ON hub.registration;
CREATE TRIGGER registration_change
    AFTER INSERT OR DELETE OR UPDATE OF metadata, software_statement, orphaned_at ON hub.registration
    FOR EACH ROW EXECUTE FUNCTION kernel.notify_change('client_id');
