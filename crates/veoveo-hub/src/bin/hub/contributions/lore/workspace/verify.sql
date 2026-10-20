UPDATE hub.lore_workspace SET default_branch = $5,
    state = CASE WHEN state = 'reserved' THEN 'verified' ELSE state END
WHERE tenant = $1 AND principal_id = $2 AND deployment = $3 AND repository = $4
    AND (default_branch IS NULL OR default_branch = $5)
RETURNING principal_id
