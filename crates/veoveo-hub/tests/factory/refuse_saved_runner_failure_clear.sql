-- A real best-effort runner_ran DELETE fails in this disposable fixture database.
-- The stale row is produced by the normal watch path, never inserted by the test.
DO $fixture$
BEGIN
    CREATE FUNCTION hub.factory_fixture_refuse_saved_clear() RETURNS trigger
        LANGUAGE plpgsql AS $refuse$
        BEGIN
            IF OLD.branch = 'links/retry-stale-clear' THEN
                RAISE EXCEPTION 'fixture refuses the saved runner failure clear';
            END IF;
            RETURN OLD;
        END;
        $refuse$;
    CREATE TRIGGER factory_fixture_refuse_saved_clear
        BEFORE DELETE ON hub.lore_runner_failure
        FOR EACH ROW EXECUTE FUNCTION hub.factory_fixture_refuse_saved_clear();
END;
$fixture$;
