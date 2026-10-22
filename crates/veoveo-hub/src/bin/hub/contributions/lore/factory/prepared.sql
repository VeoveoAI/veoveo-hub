-- The branch's latest push was prepared, which clears the failure of an earlier one.
DELETE FROM hub.lore_prepare_failure
WHERE deployment = $1 AND repository = $2 AND branch = $3
