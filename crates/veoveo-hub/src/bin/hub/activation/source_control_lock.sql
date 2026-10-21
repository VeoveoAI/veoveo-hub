SELECT r.control_plane AS "control_plane: ControlDocument"
FROM hub.control_active a JOIN hub.control_revision r ON r.id=a.revision
FOR UPDATE OF a
