SELECT id, protected_resource, oauth_client_id, work_context, scopes,
state AS "state: DeviceStatus", browser_digest, principal AS "principal: Json<Principal>", principal_display_name,
poll_interval, next_poll, expires_at FROM hub.device_authorization WHERE user_digest=$1 AND expires_at>$2 AND state='pending'
