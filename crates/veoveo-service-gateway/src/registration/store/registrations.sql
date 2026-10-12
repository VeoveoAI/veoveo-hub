SELECT
    client_id,
    server,
    metadata AS "metadata: Json<ClientMetadata>",
    software_statement,
    access_token_digest,
    issued_at
FROM gateway.registration
ORDER BY client_id
