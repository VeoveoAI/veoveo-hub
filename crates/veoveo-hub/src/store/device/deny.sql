UPDATE hub.device_authorization SET state='denied' WHERE id=$1 AND state IN ('pending','authorizing') RETURNING id
