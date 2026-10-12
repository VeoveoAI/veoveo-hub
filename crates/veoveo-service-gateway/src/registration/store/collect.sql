DELETE FROM gateway.registration AS r
WHERE r.live_at < $1
    AND NOT EXISTS (
        SELECT FROM kernel.instance AS i
        WHERE i.login = r.login::name AND kernel.instance_alive(i.id)
    )
RETURNING r.client_id, r.server, r.login
