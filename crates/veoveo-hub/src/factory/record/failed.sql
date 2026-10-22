UPDATE hub.instance
SET failed_stage = $2, failed_message = $3
WHERE deployment = $1
