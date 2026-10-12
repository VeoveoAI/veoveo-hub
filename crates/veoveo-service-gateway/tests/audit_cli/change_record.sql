UPDATE audit.record
SET draft = jsonb_set(draft, '{latency_ms}', '17')
WHERE id = $1
