UPDATE audit.block
SET block = jsonb_set(block, '{signature}', $3)
WHERE partition = $1 AND sequence = $2
