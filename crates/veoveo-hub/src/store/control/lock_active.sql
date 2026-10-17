-- Publications serialize on the single pointer row.
SELECT revision
FROM hub.control_active
FOR UPDATE
