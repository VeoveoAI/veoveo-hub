INSERT INTO hub.deployment_admission (
    deployment, environment, owner_tenant, owner_principal, server, metadata,
    storage_binding, runtime, token_digest, expires_at
)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
ON CONFLICT (deployment) DO UPDATE
SET token_digest = EXCLUDED.token_digest, expires_at = EXCLUDED.expires_at, consumed_at = NULL
