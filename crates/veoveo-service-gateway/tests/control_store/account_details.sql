-- The details of the account changes the publications audited, in append order.
SELECT draft -> 'detail' AS "detail!: Json<AuditDetail>"
FROM audit.record
WHERE class = 'account_change'
ORDER BY sequence
