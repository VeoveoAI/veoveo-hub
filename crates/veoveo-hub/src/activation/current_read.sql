SELECT r.control_plane AS "control_plane: Json<HubControlPlane>"
FROM hub.control_active a JOIN hub.control_revision r ON r.id=a.revision
