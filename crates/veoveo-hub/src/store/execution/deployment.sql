SELECT EXISTS (SELECT 1 FROM hub.deployment WHERE id = $1 AND server = $2) AS "exists!"
