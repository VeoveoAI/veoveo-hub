SELECT revision, served_at IS NOT NULL AS "served!", failed_stage, failed_message, warning
FROM hub.instance
WHERE deployment = $1
