SELECT revision, served_at IS NOT NULL AS "served!", failed_stage, failed_message, warning,
    ungranted_providers, link, linked_repository, tracked_branch, tracked_branch_name,
    linked_revision
FROM hub.instance
WHERE deployment = $1
