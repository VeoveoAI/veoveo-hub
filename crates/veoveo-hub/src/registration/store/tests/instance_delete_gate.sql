-- The first unregistered instance is removed in this disposable database. Its
-- BEFORE DELETE gate observes the real removal after its shared instance lock.
DO $fixture$
BEGIN
    CREATE FUNCTION hub.instance_removal_fixture_gate() RETURNS trigger
        LANGUAGE plpgsql AS $gate$
        BEGIN
            PERFORM pg_advisory_xact_lock(17031, 1);
            RETURN OLD;
        END;
        $gate$;
    CREATE TRIGGER instance_removal_fixture_gate BEFORE DELETE ON hub.instance
        FOR EACH ROW EXECUTE FUNCTION hub.instance_removal_fixture_gate();
END;
$fixture$;
