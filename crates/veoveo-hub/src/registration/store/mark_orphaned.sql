-- Housekeeping marks every registration whose login newly holds no live kernel
-- instance. It writes only rows whose mark changes. An instance's registration is never
-- marked: OpenShell restarts its process, and its next registration replaces it.
-- veoveo: unbounded (housekeeping checks every registration of the installation, one per running server process)
-- veoveo: scan (housekeeping reads every registration and every process instance of the installation)
UPDATE hub.registration AS r
SET orphaned_at = $1
WHERE r.orphaned_at IS NULL
    AND NOT EXISTS (
        SELECT FROM kernel.instance AS i
        WHERE i.login = r.login::name AND kernel.instance_alive(i.id)
    )
    AND NOT EXISTS (
        SELECT FROM hub.instance AS n
        WHERE n.deployment = r.deployment
    )
