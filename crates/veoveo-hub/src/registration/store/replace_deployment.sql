-- A registration of a deployment with new metadata, and each registration of an
-- instance, replaces the deployment's binding. The row lock serializes concurrent
-- registrations of one deployment.
INSERT INTO hub.deployment (id, server, metadata, storage_binding, created_at)
VALUES ($1, $2, $3, $4, $5)
ON CONFLICT (id) DO UPDATE
SET server = EXCLUDED.server, metadata = EXCLUDED.metadata, storage_binding = EXCLUDED.storage_binding
