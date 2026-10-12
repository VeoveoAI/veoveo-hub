SELECT count(*) AS "count!"
FROM gateway.control_object AS o
JOIN gateway.control_active AS a ON a.revision = o.revision
