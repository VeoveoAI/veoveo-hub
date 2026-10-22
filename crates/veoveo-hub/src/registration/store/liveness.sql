-- An instance's registration has no liveness here: its server may hold its kernel
-- instance in another database, and its health comes from its factory sandbox.
-- veoveo: scan (kernel.instance has no index on login; it holds one row per running process of the installation)
SELECT
    r.deployment,
    r.orphaned_at,
    EXISTS (
        SELECT FROM kernel.instance AS i
        WHERE i.login = r.login::name AND kernel.instance_alive(i.id)
    ) AS "alive!"
FROM hub.registration AS r
WHERE r.client_id = ANY ($1)
    AND NOT EXISTS (
        SELECT FROM hub.instance AS n
        WHERE n.deployment = r.deployment
    )
