SELECT count(*) AS "count!"
FROM hub.control_object AS o
JOIN hub.control_active AS a ON a.revision = o.revision
