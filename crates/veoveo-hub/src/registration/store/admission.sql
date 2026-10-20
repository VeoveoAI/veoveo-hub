SELECT environment, owner_tenant, owner_principal,
    metadata AS "metadata: Json<ClientMetadata>",
    storage_binding AS "storage: Json<StorageBinding>",
    runtime AS "runtime: Json<NativeUpstreamBinding>", expires_at, consumed_at
FROM hub.deployment_admission
WHERE token_digest = $1
