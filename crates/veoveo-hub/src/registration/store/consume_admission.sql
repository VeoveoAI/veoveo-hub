UPDATE hub.deployment_admission SET consumed_at = $3
WHERE deployment = $1 AND token_digest = $2 AND consumed_at IS NULL AND expires_at > clock_timestamp()
RETURNING deployment
