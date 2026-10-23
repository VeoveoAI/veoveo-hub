-- End the operation with its cause; the Computer keeps the version it had recorded.
UPDATE hub.computer
SET failure = $5, operation_id = NULL, operation_started_at = NULL,
    operation_provisioning = NULL, operation_notice_shown = false
WHERE id = $1 AND tenant = $2 AND principal_id = $3 AND operation_id = $4
RETURNING id
