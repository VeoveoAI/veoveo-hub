-- The activities of the account changes the publications audited, in append order.
SELECT draft #>> '{detail,activity}' AS "activity!"
FROM audit.record
WHERE class = 'gateway.account_change'
ORDER BY sequence
