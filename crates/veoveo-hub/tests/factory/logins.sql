SELECT login
FROM hub.registration
WHERE deployment = $1
ORDER BY login
