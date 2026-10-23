-- Begin replacing a ready Computer's sandbox with one of $5, unless another replacement
-- runs and is not yet stale. A new operation clears the last failure.
UPDATE hub.computer
SET operation_id = $4, operation_started_at = $6, operation_provisioning = $5,
    operation_notice_shown = false, failure = NULL
WHERE id = $1 AND tenant = $2 AND principal_id = $3 AND ready
  AND (operation_id IS NULL OR operation_started_at < $7)
RETURNING id
