SELECT
    token_digest,
    generation,
    consumed_at,
    delivery_envelope IS NOT NULL AS "delivered!",
    delivery_expires_at
FROM gateway.refresh_token
ORDER BY generation
