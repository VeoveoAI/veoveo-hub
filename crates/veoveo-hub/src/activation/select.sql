UPDATE hub.activation_head SET serving = $5
WHERE owner_tenant = $1 AND owner_principal = $2 AND environment = $3 AND component = $4
    AND desired = $5 AND generation = $6
