-- veoveo: unbounded (every instance of the factory, each with a sandbox the installation runs)
SELECT instance.deployment, deployment.server AS "server?", instance.repository, instance.branch,
    instance.database, instance.created_at, instance.served_at, instance.pusher, instance.linked_pusher,
    instance.revision, instance.served_at IS NOT NULL AS "served!", instance.failed_stage,
    instance.failed_message, instance.warning, instance.ungranted_providers, instance.link,
    instance.linked_repository, instance.tracked_branch, instance.tracked_branch_name,
    instance.linked_revision
FROM hub.instance AS instance
LEFT JOIN hub.deployment AS deployment ON deployment.id = instance.deployment
ORDER BY instance.repository, instance.branch, instance.link
