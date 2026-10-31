-- The providers an instance's revision requested in openshell/providers.yaml that the
-- installation does not grant its repository, which its status names.
ALTER TABLE hub.instance
    ADD COLUMN ungranted_providers text[] NOT NULL DEFAULT '{}';
COMMENT ON COLUMN hub.instance.ungranted_providers IS
    'The providers the last applied or attempted revision requested and the installation does not grant the instance''s repository.';
