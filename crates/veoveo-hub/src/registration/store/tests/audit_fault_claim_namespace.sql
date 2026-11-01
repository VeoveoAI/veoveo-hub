-- Test-only utility statement: the selector exists only after refuse_audit.sql.
-- Preparing/planning this DO needs no fixture relation in the migrated schema.
DO $fixture$
DECLARE
    changed bigint;
BEGIN
    UPDATE hub.registration_fixture_audit_fault
    SET activity = 'claim_namespace'
    WHERE singleton;
    GET DIAGNOSTICS changed = ROW_COUNT;
    IF changed <> 1 THEN
        RAISE EXCEPTION 'expected exactly one fixture audit selector, changed %', changed;
    END IF;
END;
$fixture$;
