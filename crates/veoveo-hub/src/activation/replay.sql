SELECT accepted AS "accepted: Json<AcceptedApply>"
FROM hub.activation_build
WHERE owner_tenant = $1 AND owner_principal = $2 AND environment = $3 AND component = $4 AND idempotency = $5
