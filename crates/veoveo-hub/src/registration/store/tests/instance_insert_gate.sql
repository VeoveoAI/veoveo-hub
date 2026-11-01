-- The service generates the client identifier. This disposable test database has
-- exactly one registration attempt, so its actual INSERT is the gate identity.
DO $fixture$
BEGIN
    CREATE FUNCTION hub.instance_registration_fixture_gate() RETURNS trigger
        LANGUAGE plpgsql AS $gate$
        BEGIN
            PERFORM pg_advisory_xact_lock(17031, 1);
            RETURN NEW;
        END;
        $gate$;
    CREATE TRIGGER instance_registration_fixture_gate BEFORE INSERT ON hub.registration
        FOR EACH ROW EXECUTE FUNCTION hub.instance_registration_fixture_gate();
END;
$fixture$;
