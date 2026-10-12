DELETE gateway_registration WHERE lease_expires_at <= $now RETURN BEFORE;
