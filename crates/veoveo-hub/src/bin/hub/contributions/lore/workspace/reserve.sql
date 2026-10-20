INSERT INTO hub.lore_workspace (tenant, principal_id, deployment, repository, default_branch, name, mode, publication_id)
VALUES ($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT (tenant, principal_id) DO NOTHING
