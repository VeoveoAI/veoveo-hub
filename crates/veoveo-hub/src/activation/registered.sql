SELECT d.metadata AS "metadata: Json<veoveo_hub_sdk::registration::ClientMetadata>",
    d.storage_binding AS "storage: Json<veoveo_hub_sdk::registration::StorageBinding>",
    d.runtime AS "runtime: Json<crate::control_plane::NativeUpstreamBinding>",
    d.environment, d.owner_tenant, d.owner_principal,
    EXISTS (SELECT FROM hub.registration r WHERE r.deployment = d.id) AS "registered!"
FROM hub.deployment d WHERE d.id = $1 AND d.server = $2
