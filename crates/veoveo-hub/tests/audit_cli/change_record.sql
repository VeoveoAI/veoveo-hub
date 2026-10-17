UPDATE audit.record
SET draft = jsonb_set(draft, '{detail,method}', '"prompt_get"')
WHERE id = $1
