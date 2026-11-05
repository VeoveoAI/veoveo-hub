-- Registrations join when an instance's server last came up. A server that registers or
-- resumes may serve other skills under the same declaration, so its coming up starts a
-- new generation. Its stopping does not: what it served stays listed until it comes back.
CREATE TRIGGER instance_served_change
    AFTER UPDATE OF served_at ON hub.instance
    FOR EACH ROW
    WHEN (NEW.served_at IS NOT NULL AND OLD.served_at IS DISTINCT FROM NEW.served_at)
    EXECUTE FUNCTION kernel.notify_change('deployment');
