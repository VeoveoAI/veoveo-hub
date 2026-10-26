SELECT principal, display_name, email
FROM hub.person
WHERE tenant = $1 AND principal = ANY($2)
