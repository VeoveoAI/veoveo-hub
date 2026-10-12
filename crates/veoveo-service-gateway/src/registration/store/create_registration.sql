INSERT INTO gateway.registration (
    client_id, server, metadata, software_statement, access_token_digest, login,
    issued_at, updated_at, live_at
)
VALUES ($1, $2, $3, $4, $5, $6, $7, $7, $7)
