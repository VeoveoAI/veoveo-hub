SELECT i.id AS "id!"
FROM kernel.instance AS i
WHERE i.server = 'hub' AND kernel.instance_alive(i.id)
ORDER BY i.id
