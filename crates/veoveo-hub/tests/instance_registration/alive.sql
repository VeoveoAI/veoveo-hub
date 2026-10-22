SELECT EXISTS (
    SELECT FROM kernel.instance AS i
    WHERE i.login = $1::name AND kernel.instance_alive(i.id)
) AS "alive!"
