INSERT INTO hub.computer (id, tenant, principal_id, workspace_request, template_request, model_provider_request, hub_provider_request, sandbox_request, created_at)
VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
ON CONFLICT (tenant, principal_id) DO NOTHING
