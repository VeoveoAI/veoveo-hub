SELECT
    (SELECT to_jsonb(i) FROM hub.instance AS i WHERE i.deployment = $1) AS "instance",
    (SELECT coalesce(jsonb_agg(r.draft ORDER BY r.sequence), '[]'::jsonb)
       FROM audit.record AS r
      WHERE r.class = 'hub.registration' AND r.target = $2) AS "records!"
