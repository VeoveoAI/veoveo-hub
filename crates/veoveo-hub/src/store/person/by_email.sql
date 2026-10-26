SELECT principal, display_name
FROM hub.person
WHERE tenant = $1 AND email = $2
