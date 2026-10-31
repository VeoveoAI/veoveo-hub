SELECT computer_id, family_id FROM hub.app_origin_session WHERE digest = $1 AND server = $2
