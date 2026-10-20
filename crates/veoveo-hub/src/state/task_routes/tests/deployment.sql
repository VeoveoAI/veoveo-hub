WITH claim AS (
    INSERT INTO hub.namespace_claim (server, key_thumbprint, claimed_at)
    VALUES ($2, $3, clock_timestamp())
    ON CONFLICT (server) DO UPDATE SET server = EXCLUDED.server
    RETURNING server
)
INSERT INTO hub.deployment (id, server, metadata, storage_binding, created_at)
SELECT $1, claim.server, $4, $5, clock_timestamp() FROM claim
