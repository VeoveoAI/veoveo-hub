SELECT h.component, b.accepted AS "accepted: Json<AcceptedApply>", s.document AS "document: Json<SourceRecord>"
FROM hub.activation_head h
JOIN hub.activation_build b ON b.id=h.desired AND b.generation=h.generation
JOIN hub.activation_source s ON s.build=b.id
WHERE h.owner_tenant=$1 AND h.owner_principal=$2 AND h.environment=$3 AND h.component>$4
    AND b.owner_tenant=$1 AND b.owner_principal=$2 AND b.environment=$3
    AND s.document->'component'->'source'->>'kind'='lore'
    AND s.document->'component'->'source'->'selection'->>'kind'='follow'
ORDER BY h.component
LIMIT $5
