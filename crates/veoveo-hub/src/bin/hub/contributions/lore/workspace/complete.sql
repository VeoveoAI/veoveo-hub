UPDATE hub.lore_workspace SET state = 'ready'
WHERE tenant = $1 AND principal_id = $2 AND deployment = $3 AND repository = $4
    AND state IN ('verified', 'ready') RETURNING principal_id
