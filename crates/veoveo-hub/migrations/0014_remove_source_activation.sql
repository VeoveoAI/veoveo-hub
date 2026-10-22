-- Development hard cut: factory sandboxes supersede environment source activation
-- and deployment-scoped registration admission. Reset application data instead of
-- migrating it.
DROP TABLE hub.activation_worker;
DROP TABLE hub.activation_runtime;
DROP TABLE hub.activation_source;
DROP TABLE hub.activation_head;
DROP TABLE hub.activation_build;
DROP TABLE hub.environment_apply;
DROP TABLE hub.environment_definition_source;
DROP INDEX hub.task_route_tenant_deployment_expiry;

DROP TABLE hub.deployment_admission;
ALTER TABLE hub.deployment
    DROP COLUMN environment,
    DROP COLUMN owner_tenant,
    DROP COLUMN owner_principal,
    DROP COLUMN runtime;
