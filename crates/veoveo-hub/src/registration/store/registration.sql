SELECT
    registration.client_id,
    registration.server,
    registration.metadata AS "metadata: Json<ClientMetadata>",
    registration.software_statement,
    registration.access_token_digest,
    registration.issued_at,
    instance.branch AS "branch?"
FROM hub.registration
LEFT JOIN hub.instance ON instance.deployment = registration.deployment
WHERE registration.client_id = $1
