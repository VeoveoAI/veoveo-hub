-- Development hard cut: reset old Task routes instead of inventing a capture.
ALTER TABLE hub.task_route
    ADD COLUMN deployment uuid NOT NULL REFERENCES hub.deployment (id),
    ADD COLUMN environment uuid NOT NULL,
    ADD COLUMN composition text NOT NULL REFERENCES hub.execution_composition (revision);
ALTER TABLE hub.task_route DROP CONSTRAINT task_route_server_source_task_id_owner_profile_key;
ALTER TABLE hub.task_route ADD UNIQUE (server, deployment, source_task_id, owner, profile);
CREATE INDEX task_route_deployment ON hub.task_route (deployment);
CREATE INDEX task_route_composition ON hub.task_route (composition);
COMMENT ON COLUMN hub.task_route.composition IS
    'The immutable execution capture at Task creation; current routing edits cannot retarget retained work.';
