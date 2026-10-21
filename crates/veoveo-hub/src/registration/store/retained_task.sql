-- Environment access requires its owner tenant; authenticated WorkContext and
-- delegated authority retain that same caller tenant. $1 is its canonical identity
-- UUID from Core deterministic_tenant_id, exactly as on route writes.
SELECT EXISTS (SELECT 1 FROM hub.task_route
    WHERE tenant=$1 AND deployment=$2 AND expires_at>$3) AS "retained!"
