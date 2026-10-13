UPDATE gateway.registration AS r
SET orphaned_at = NULL
WHERE r.orphaned_at IS NOT NULL
    AND EXISTS (
        SELECT FROM kernel.instance AS i
        WHERE i.login = r.login::name AND kernel.instance_alive(i.id)
    )
