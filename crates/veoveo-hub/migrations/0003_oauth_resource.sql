-- Development hard cut: discard hosted OAuth sessions, never infer their authority
-- from a later catalog revision. Computer owner/workspace mappings and their native
-- working storage remain; their next ordinary login performs a fresh custody handoff.
DELETE FROM hub.authorization_request;
DELETE FROM hub.authorization_code;
DELETE FROM hub.computer_login;
DELETE FROM hub.computer_browser_session;
UPDATE hub.computer SET ready = false, family_id = NULL, handoff_id = NULL,
    configure_request = NULL, rotate_request = NULL;
DELETE FROM hub.refresh_token;
DELETE FROM hub.refresh_family;

ALTER TABLE hub.authorization_request DROP COLUMN profile;
ALTER TABLE hub.authorization_request ADD COLUMN protected_resource text NOT NULL CHECK (protected_resource <> '');
ALTER TABLE hub.authorization_code DROP COLUMN profile;
ALTER TABLE hub.authorization_code ADD COLUMN protected_resource text NOT NULL CHECK (protected_resource <> '');
ALTER TABLE hub.refresh_family DROP COLUMN profile;
ALTER TABLE hub.refresh_family ADD COLUMN protected_resource text NOT NULL CHECK (protected_resource <> '');
