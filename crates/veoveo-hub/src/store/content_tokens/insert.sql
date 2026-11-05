INSERT INTO hub.content_token
    (digest, nonce, session_digest, owner, item, computer_id, family_id, expires_at, hard_expires_at)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
