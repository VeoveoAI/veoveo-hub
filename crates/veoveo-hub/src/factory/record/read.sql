SELECT revision, failed_stage, failed_message, warning
FROM hub.instance
WHERE deployment = $1
