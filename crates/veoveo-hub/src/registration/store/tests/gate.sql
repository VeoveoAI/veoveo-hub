DO $fixture$
BEGIN
    CREATE FUNCTION hub.registration_fixture_gate() RETURNS trigger
        LANGUAGE plpgsql AS $gate$
        BEGIN
            IF NEW.client_id = 'registration-probe-00000000000002' THEN
                PERFORM pg_advisory_xact_lock(17031, 1);
            END IF;
            RETURN NEW;
        END;
        $gate$;
    CREATE TRIGGER registration_fixture_gate BEFORE INSERT ON hub.registration
        FOR EACH ROW EXECUTE FUNCTION hub.registration_fixture_gate();
END;
$fixture$;
