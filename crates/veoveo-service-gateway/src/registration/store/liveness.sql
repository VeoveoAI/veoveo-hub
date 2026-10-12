SELECT
    r.server,
    r.live_at,
    EXISTS (
        SELECT FROM kernel.instance AS i
        WHERE i.login = r.login::name AND kernel.instance_alive(i.id)
    ) AS "alive!"
FROM gateway.registration AS r
WHERE r.client_id = ANY ($1)
