-- Environment access requires its owner tenant; authenticated WorkContext and
-- delegated authority retain that same caller tenant. $1 is its control text key;
-- $5 is the canonical identity UUID from Core deterministic_tenant_id, as on route writes.
SELECT b.id, b.accepted AS "accepted: Json<AcceptedApply>", b.state AS "state: Json<BuildState>"
FROM hub.activation_build b
WHERE b.owner_tenant=$1 AND b.owner_principal=$2 AND b.environment=$3 AND b.id>$4
    AND (
        b.state->>'phase' IN ('serving','registering')
        OR (
            b.state->>'phase'='failed'
            AND b.state #>> '{failure,stage}'='worker'
            AND b.state #>> '{failure,code}'='native_unavailable'
        )
    )
    AND EXISTS (SELECT 1 FROM hub.task_route r
        WHERE r.tenant=$5 AND r.deployment=b.deployment AND r.expires_at>$6)
    AND NOT EXISTS (SELECT 1 FROM hub.activation_head h WHERE h.desired=b.id AND h.generation=b.generation)
ORDER BY b.id
LIMIT $7
