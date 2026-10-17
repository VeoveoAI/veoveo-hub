UPDATE hub.refresh_family
SET issued_at = $2::timestamptz - interval '1 hour', expires_at = $2
WHERE id = $1
