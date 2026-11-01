-- Registrations join an instance's branch and linked routing identity. Retargeting
-- can change that output without changing any process registration's metadata.
CREATE TRIGGER instance_routing_change
    AFTER UPDATE OF branch, link, linked_repository ON hub.instance
    FOR EACH ROW
    WHEN ((OLD.branch, OLD.link, OLD.linked_repository)
        IS DISTINCT FROM (NEW.branch, NEW.link, NEW.linked_repository))
    EXECUTE FUNCTION kernel.notify_change('deployment');
