SELECT nonce FROM hub.content_token
WHERE session_digest = $1 AND owner = $2 AND item = $3 AND family_id = $4 AND expires_at > $5
ORDER BY expires_at DESC, digest
LIMIT 1
