SELECT document AS "document: Json<ApplyReceipt>" FROM hub.environment_apply
WHERE owner_tenant=$1 AND owner_principal=$2 AND environment=$3 AND idempotency=$4
