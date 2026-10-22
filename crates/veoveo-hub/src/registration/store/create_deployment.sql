INSERT INTO hub.deployment (id, server, metadata, storage_binding, created_at)
VALUES ($1, $2, $3, $4, $5)
ON CONFLICT (id) DO NOTHING
