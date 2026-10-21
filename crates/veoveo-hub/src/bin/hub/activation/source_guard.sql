SELECT b.accepted AS "accepted: Json<AcceptedApply>", h.desired = b.id AND h.generation = b.generation AS "current!"
FROM hub.activation_build b
JOIN hub.activation_head h ON h.owner_tenant=b.owner_tenant AND h.owner_principal=b.owner_principal
    AND h.environment=b.environment AND h.component=b.component
WHERE b.id=$1
FOR UPDATE OF h
