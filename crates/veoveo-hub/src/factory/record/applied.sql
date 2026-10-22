-- The Hub applied a revision to the instance's sandbox, which clears its last failure.
UPDATE hub.instance
SET revision = $2, failed_stage = NULL, failed_message = NULL
WHERE deployment = $1
