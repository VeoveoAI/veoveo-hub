-- One page of a tenant's daily usage totals for the days in [$3, $4), in day,
-- principal, server, kind, phase and currency order, after the position $5..$10 when
-- one is given. $2 limits the page to one principal's totals.
SELECT
    day AS "day!",
    principal AS "principal!",
    source AS "source!",
    kind AS "kind!",
    phase AS "phase!",
    currency AS "currency!",
    quantity::text AS "quantity!",
    amount::text AS "amount!",
    records AS "records!"
FROM usage.daily
WHERE tenant = $1
    AND ($2::text IS NULL OR principal = $2)
    AND day >= $3 AND day < $4
    AND ($5::date IS NULL
        OR (day, principal, source, kind, phase, currency) > ($5, $6, $7, $8, $9, $10))
ORDER BY day, principal, source, kind, phase, currency
LIMIT $11
