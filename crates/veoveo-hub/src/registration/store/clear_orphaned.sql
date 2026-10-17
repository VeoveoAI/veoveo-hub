-- Housekeeping clears the orphan mark of every registration whose login holds a live
-- kernel instance again. It writes only rows whose mark changes.
-- veoveo: unbounded (housekeeping checks every registration of the installation, one per running server process)
-- veoveo: scan (housekeeping reads every registration and every process instance of the installation)
UPDATE hub.registration AS r
SET orphaned_at = NULL
WHERE r.orphaned_at IS NOT NULL
    AND EXISTS (
        SELECT FROM kernel.instance AS i
        WHERE i.login = r.login::name AND kernel.instance_alive(i.id)
    )
