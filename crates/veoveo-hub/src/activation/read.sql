SELECT b.accepted AS "accepted: Json<AcceptedApply>", b.state AS "state: Json<BuildState>",
    h.desired = b.id AND h.generation = b.generation AS "current!", s.id AS serving_build, s.deployment AS serving_deployment
FROM hub.activation_build b
JOIN hub.activation_head h ON h.owner_tenant = b.owner_tenant AND h.owner_principal = b.owner_principal
    AND h.environment = b.environment AND h.component = b.component
LEFT JOIN hub.activation_build s ON s.id = h.serving
WHERE b.id = $1 AND b.owner_tenant = $2 AND b.owner_principal = $3
