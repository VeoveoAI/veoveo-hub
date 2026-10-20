SELECT metadata AS "metadata: Json<ClientMetadata>", storage_binding AS "storage: Json<StorageBinding>",
    environment, owner_tenant, owner_principal, runtime AS "runtime: Json<NativeUpstreamBinding>"
FROM hub.deployment WHERE id = $1
