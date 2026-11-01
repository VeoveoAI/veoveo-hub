-- Serialize instance lifetime before registration admission or removal's DELETE
-- snapshot. The deployment row may not exist before the first registration.
SELECT deployment
FROM hub.instance
WHERE deployment = $1
FOR UPDATE
