-- A completed sign-in, without its refresh family or browser session.
UPDATE hub.computer SET ready = true, family_id = $2 WHERE id = $1 AND tenant = $3
