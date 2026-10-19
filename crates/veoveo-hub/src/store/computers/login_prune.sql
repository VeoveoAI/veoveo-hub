DELETE FROM hub.computer_login WHERE state_digest IN (
    SELECT state_digest FROM hub.computer_login WHERE expires_at <= $1 ORDER BY expires_at LIMIT 1000
)
