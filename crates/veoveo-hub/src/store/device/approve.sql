UPDATE hub.device_authorization SET state='approved', principal=$3, principal_display_name=$4
WHERE id=$1 AND browser_digest=$2 AND state='authorizing' AND expires_at>$5 RETURNING id
