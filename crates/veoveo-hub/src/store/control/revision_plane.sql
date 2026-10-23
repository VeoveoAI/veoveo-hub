SELECT control_plane AS "control_plane: Json<HubControlPlane>"
FROM hub.control_revision
WHERE id = $1
