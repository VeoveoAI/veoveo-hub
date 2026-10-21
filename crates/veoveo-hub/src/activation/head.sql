SELECT generation, desired, serving FROM hub.activation_head
WHERE owner_tenant = $1 AND owner_principal = $2 AND environment = $3 AND component = $4
FOR UPDATE
