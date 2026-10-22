SELECT server, metadata AS "metadata: Json<ClientMetadata>", storage_binding AS "storage: Json<StorageBinding>"
FROM hub.deployment WHERE id = $1
