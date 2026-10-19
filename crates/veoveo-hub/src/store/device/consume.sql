UPDATE hub.device_authorization SET state='consumed' WHERE id=$1 AND state='approved' RETURNING id
