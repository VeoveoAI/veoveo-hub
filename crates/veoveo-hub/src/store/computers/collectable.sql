-- Every ready Computer with no replacement running, or only one that began before $1.
-- veoveo: all tenants (collection sweeps the Computers of every tenant; each is then claimed by its own tenant and principal)
-- veoveo: unbounded (one row per person with a Computer in the installation, read once a sweep)
-- veoveo: scan (the sweep reads every ready Computer of the installation every five minutes)
SELECT id, tenant, principal_id, workspace_request, model_provider_request, hub_provider_request, handoff_id, configure_request, rotate_request, family_id, ready, provisioning AS "provisioning: Json<ComputerProvisionSpec>", operation_id, operation_started_at, operation_provisioning AS "operation_provisioning: Json<ComputerProvisionSpec>", operation_notice_shown, failure AS "failure: Json<ComputerFailure>", update_notice AS "update_notice: Json<UpdateNotice>", collected_at FROM hub.computer WHERE ready AND (operation_id IS NULL OR operation_started_at < $1) ORDER BY id
