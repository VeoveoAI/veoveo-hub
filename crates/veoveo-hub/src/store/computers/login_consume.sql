DELETE FROM hub.computer_login WHERE state_digest = $1 AND browser_digest = $2 AND expires_at > $3 RETURNING verifier
