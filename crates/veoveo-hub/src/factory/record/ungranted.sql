UPDATE hub.instance
SET ungranted_providers = $2
WHERE deployment = $1
