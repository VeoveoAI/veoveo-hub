-- End a collection: when it deleted the sandbox, record when ($5); a collection that did
-- not leaves the Computer as it was, with no failure, for the next sweep.
UPDATE hub.computer
SET collected_at = COALESCE($5, collected_at), operation_id = NULL,
    operation_started_at = NULL, operation_provisioning = NULL, operation_notice_shown = false
WHERE id = $1 AND tenant = $2 AND principal_id = $3 AND operation_id = $4
RETURNING id
