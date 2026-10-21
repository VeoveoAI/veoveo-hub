-- Tenant-scoped keyset walks of retained builds and exact canonical Task probes.
CREATE INDEX activation_build_owner_page
    ON hub.activation_build(owner_tenant, owner_principal, environment, id);
CREATE INDEX task_route_tenant_deployment_expiry
    ON hub.task_route(tenant, deployment, expires_at);
COMMENT ON TABLE hub.activation_runtime IS 'Retention: exact native runtime provenance remains while desired, serving or canonical retained Task workers depend on its workspace; no automatic reclamation is implemented and source updates do not replace this record.';
COMMENT ON TABLE hub.activation_worker IS 'Retention: exact per-build native listener placement remains with immutable source/output while desired, serving or unexpired canonical Task routes/results require that worker; no automatic reclamation is implemented and retention never replays a Task on another deployment.';
