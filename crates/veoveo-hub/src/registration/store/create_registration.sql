INSERT INTO hub.registration (
    client_id, deployment, server, metadata, software_statement, access_token_digest, login,
    issued_at, updated_at
)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $8)
