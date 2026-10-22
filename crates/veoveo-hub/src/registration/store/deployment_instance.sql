SELECT EXISTS (SELECT FROM hub.instance WHERE deployment = $1) AS "instance!"
