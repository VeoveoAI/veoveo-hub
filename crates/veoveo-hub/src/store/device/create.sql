INSERT INTO hub.device_authorization (id,device_digest,user_digest,protected_resource,oauth_client_id,scopes,state,poll_interval,next_poll,created_at,expires_at)
VALUES ($1,$2,$3,$4,$5,$6,'pending',5,$7,$8,$9)
