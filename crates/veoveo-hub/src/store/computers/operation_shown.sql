UPDATE hub.computer SET operation_notice_shown = true
WHERE id = $1 AND tenant = $2 AND principal_id = $3 AND operation_id = $4
