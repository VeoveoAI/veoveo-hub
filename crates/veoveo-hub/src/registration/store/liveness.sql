-- veoveo: scan (kernel.instance has no index on login; it holds one row per running process of the installation)
SELECT
    r.server,
    r.orphaned_at,
    EXISTS (
        SELECT FROM kernel.instance AS i
        WHERE i.login = r.login::name AND kernel.instance_alive(i.id)
    ) AS "alive!"
FROM hub.registration AS r
WHERE r.client_id = ANY ($1)
