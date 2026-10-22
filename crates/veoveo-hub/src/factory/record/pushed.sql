SELECT count(*) AS "instances!"
FROM hub.instance
WHERE pusher = $1
