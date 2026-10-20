INSERT INTO hub.deployment (id, server, metadata, storage_binding, created_at, environment, owner_tenant, owner_principal, runtime)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
ON CONFLICT (id) DO NOTHING
