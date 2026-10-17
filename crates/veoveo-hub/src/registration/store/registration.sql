SELECT
    client_id,
    server,
    metadata AS "metadata: Json<ClientMetadata>",
    software_statement,
    access_token_digest,
    issued_at
FROM hub.registration
WHERE client_id = $1
