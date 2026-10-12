DELETE FROM audit.block
WHERE partition = $1 AND sequence = $2
