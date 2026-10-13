-- Housekeeping removes every registration orphaned past the grace period. The caller
-- drops each removed registration's login in the same transaction.
-- veoveo: unbounded (the orphaned registrations of ended server processes, which one transaction removes with their logins)
-- veoveo: scan (housekeeping reads every registration and every process instance of the installation)
DELETE FROM gateway.registration AS r
WHERE r.orphaned_at < $1
    AND NOT EXISTS (
        SELECT FROM kernel.instance AS i
        WHERE i.login = r.login::name AND kernel.instance_alive(i.id)
    )
RETURNING r.client_id, r.server, r.login
