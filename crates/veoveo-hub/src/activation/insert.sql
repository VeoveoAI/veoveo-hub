INSERT INTO hub.activation_build
    (id, deployment, owner_tenant, owner_principal, environment, component, generation, idempotency, accepted, state, created_at, updated_at)
VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,'{"phase":"preparing"}'::jsonb,$10,$10)
