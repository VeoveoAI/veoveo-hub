-- What a revision's activation changes outside the Hub, such as renewing the Lore
-- access of Computers whose owners' assignments changed, happens once per
-- installation: the first Hub process that sees a revision active past this pointer
-- claims the change from it and applies it.
ALTER TABLE hub.control_active
    ADD COLUMN activated_revision text REFERENCES hub.control_revision (id);
COMMENT ON COLUMN hub.control_active.activated_revision IS
    'The last active revision whose activation a Hub process claimed; a process claims the change from it to the active revision once.';

-- A claim changes no active revision, so it notifies no replica.
DROP TRIGGER control_active_changed ON hub.control_active;
CREATE TRIGGER control_active_changed
    AFTER UPDATE OF revision ON hub.control_active
    FOR EACH ROW EXECUTE FUNCTION kernel.notify_change('revision');

-- An existing installation has applied its active revision.
UPDATE hub.control_active SET activated_revision = revision;
