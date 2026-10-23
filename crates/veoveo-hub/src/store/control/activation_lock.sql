-- Claims serialize with publications on the single pointer row.
SELECT revision, activated_revision
FROM hub.control_active
FOR UPDATE
