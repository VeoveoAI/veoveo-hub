-- The owner keeps their Computer on, or no longer does.
UPDATE hub.computer SET keep_on = $4 WHERE id = $1 AND tenant = $2 AND principal_id = $3
