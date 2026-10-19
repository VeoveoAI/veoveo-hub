-- Native notifications are delivered only after the publication transaction commits.
CREATE TRIGGER control_active_changed
    AFTER UPDATE ON hub.control_active
    FOR EACH ROW EXECUTE FUNCTION kernel.notify_change('revision');
