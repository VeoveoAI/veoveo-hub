DELETE FROM hub.instance
WHERE deployment = $1
RETURNING database
