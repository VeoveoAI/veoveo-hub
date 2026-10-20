SELECT r.control_plane AS "control_plane: Json<HubControlPlane>"
FROM hub.control_active AS a
JOIN hub.control_revision AS r ON r.id = a.revision
FOR SHARE OF a
