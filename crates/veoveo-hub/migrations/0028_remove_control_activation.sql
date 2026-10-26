-- No Hub process claims a revision's activation any longer: an assignment change reaches
-- each Computer as its next repository token, so nothing outside the Hub is applied once
-- per installation. The trigger 0022 narrowed to revision changes stays as it is.
ALTER TABLE hub.control_active DROP COLUMN activated_revision;
