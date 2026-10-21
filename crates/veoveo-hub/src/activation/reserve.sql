INSERT INTO hub.activation_head (owner_tenant, owner_principal, environment, component, generation, desired)
VALUES ($1,$2,$3,$4,$5,$6)
ON CONFLICT (owner_tenant, owner_principal, environment, component)
DO UPDATE SET generation = excluded.generation, desired = excluded.desired
