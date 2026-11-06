SELECT EXISTS (SELECT 1 FROM hub.instance WHERE deployment = $1) AS "instance!"
