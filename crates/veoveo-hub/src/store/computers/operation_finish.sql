-- Record the version the operation created, and its update notice unless the start page
-- showed the operation's changes. A Computer with a sandbox is no longer collected.
UPDATE hub.computer
SET provisioning = operation_provisioning,
    update_notice = CASE WHEN operation_notice_shown THEN NULL ELSE $5::jsonb END,
    operation_id = NULL, operation_started_at = NULL, operation_provisioning = NULL,
    operation_notice_shown = false, collected_at = NULL
WHERE id = $1 AND tenant = $2 AND principal_id = $3 AND operation_id = $4
RETURNING id
