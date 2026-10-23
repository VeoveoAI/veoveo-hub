INSERT INTO hub.computer (id, tenant, principal_id, workspace_request, model_provider_request, hub_provider_request, created_at, provisioning)
VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
ON CONFLICT (tenant, principal_id) DO NOTHING
