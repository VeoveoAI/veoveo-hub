UPDATE hub.content_token SET expires_at = LEAST($2, hard_expires_at) WHERE digest = $1
