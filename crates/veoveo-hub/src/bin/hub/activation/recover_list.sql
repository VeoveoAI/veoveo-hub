SELECT h.component, b.accepted AS "accepted: Json<AcceptedApply>", b.state AS "state: Json<BuildState>",
    s.document AS "document: Json<SourceRecord>"
FROM hub.activation_head h
JOIN hub.activation_build b ON b.id=h.desired AND b.generation=h.generation
JOIN hub.activation_source s ON s.build=b.id
WHERE h.owner_tenant=$1 AND h.owner_principal=$2 AND h.environment=$3 AND h.component>$4
    AND b.owner_tenant=$1 AND b.owner_principal=$2 AND b.environment=$3
    AND b.state->>'phase' IN ('preparing','source_ready','building','registering','serving','failed')
ORDER BY h.component
LIMIT $5
