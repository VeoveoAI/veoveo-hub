SELECT tenant, principal_id, deployment, repository, default_branch, name, mode, state, publication_id, last_status, computer_id
FROM hub.lore_workspace WHERE tenant = $1 AND principal_id = $2
