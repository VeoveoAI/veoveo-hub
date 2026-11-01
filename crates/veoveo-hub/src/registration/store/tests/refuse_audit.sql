-- Test-only selection of one existing activity. Each test owns its database;
-- null disables refusal. No production field, schema migration or hook is used.
DO $fixture$
BEGIN
    CREATE TABLE hub.registration_fixture_audit_fault (
        singleton boolean PRIMARY KEY DEFAULT true CHECK (singleton),
        activity text
    );
    INSERT INTO hub.registration_fixture_audit_fault (singleton) VALUES (true);
    CREATE FUNCTION hub.registration_fixture_refuse_audit() RETURNS trigger
        LANGUAGE plpgsql AS $refuse$
        BEGIN
            IF NEW.draft ->> 'class' = 'hub.registration' AND EXISTS (
                SELECT FROM hub.registration_fixture_audit_fault AS fault
                WHERE fault.activity = NEW.draft #>> '{detail,activity}'
            ) THEN
                RAISE EXCEPTION 'fixture refuses registration audit %', NEW.draft #>> '{detail,activity}';
            END IF;
            RETURN NEW;
        END;
        $refuse$;
    CREATE TRIGGER registration_fixture_refuse_audit
        BEFORE INSERT ON audit.record
        FOR EACH ROW EXECUTE FUNCTION hub.registration_fixture_refuse_audit();
END;
$fixture$;
