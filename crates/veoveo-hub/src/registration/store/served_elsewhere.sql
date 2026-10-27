-- Whether a registration serves the slug from an instance of another repository than the
-- one whose instance has the deployment.
SELECT EXISTS (
    SELECT FROM hub.registration
    JOIN hub.instance AS other ON other.deployment = registration.deployment
    JOIN hub.instance AS own ON own.deployment = $2
    WHERE registration.server = $1 AND other.repository <> own.repository
) AS "elsewhere!"
