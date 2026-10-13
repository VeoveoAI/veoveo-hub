UPDATE gateway.registration AS r
SET orphaned_at = $1
WHERE r.orphaned_at IS NULL
    AND NOT EXISTS (
        SELECT FROM kernel.instance AS i
        WHERE i.login = r.login::name AND kernel.instance_alive(i.id)
    )
