UPDATE hub.device_authorization SET state='authorizing', browser_digest=$3 WHERE id=$1 AND user_digest=$2 AND state='pending' AND expires_at>$4 RETURNING id, protected_resource, oauth_client_id, work_context, scopes,
state AS "state: DeviceStatus", browser_digest, principal AS "principal: Json<Principal>", principal_display_name,
poll_interval, next_poll, expires_at
