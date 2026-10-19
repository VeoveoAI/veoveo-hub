UPDATE hub.refresh_token
SET delivery_envelope = NULL, delivery_expires_at = NULL
WHERE family = $1
