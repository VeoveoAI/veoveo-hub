-- Only the service's post-admission UPDATE names served_at. Installation fixture
-- has one instance and no Factory/watcher; admission's store transaction does not
-- execute this UPDATE. The trigger exists only in this disposable test database.
DO $fixture$
BEGIN
    CREATE FUNCTION hub.registration_fixture_refuse_served() RETURNS trigger
        LANGUAGE plpgsql AS $refuse$
        BEGIN
            RAISE EXCEPTION 'fixture refuses postcommit served marker';
        END;
        $refuse$;
    CREATE TRIGGER registration_fixture_refuse_served
        BEFORE UPDATE OF served_at ON hub.instance
        FOR EACH ROW EXECUTE FUNCTION hub.registration_fixture_refuse_served();
END;
$fixture$;
