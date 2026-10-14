SELECT i.id AS "id!"
FROM kernel.instance AS i
WHERE i.server = 'gateway' AND kernel.instance_alive(i.id)
ORDER BY i.id
