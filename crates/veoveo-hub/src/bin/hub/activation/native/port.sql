SELECT min(port) AS port FROM generate_series(30000,59999) AS candidate(port)
WHERE NOT EXISTS(SELECT 1 FROM hub.activation_worker w WHERE w.runtime=$1 AND w.port=candidate.port)
