UPDATE gateway.registration AS r
SET live_at = $1
WHERE EXISTS (
    SELECT FROM kernel.instance AS i
    WHERE i.login = r.login::name AND kernel.instance_alive(i.id)
)
