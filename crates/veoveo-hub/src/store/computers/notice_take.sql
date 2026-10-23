-- Return the update notice once and clear it.
WITH shown AS MATERIALIZED (
    SELECT id, update_notice FROM hub.computer
    WHERE id = $1 AND tenant = $2 AND principal_id = $3 AND update_notice IS NOT NULL
    FOR UPDATE
)
UPDATE hub.computer AS computer SET update_notice = NULL
FROM shown WHERE computer.id = shown.id
RETURNING shown.update_notice AS "update_notice!: Json<UpdateNotice>"
