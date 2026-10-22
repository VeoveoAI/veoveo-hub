SELECT EXISTS (SELECT FROM hub.instance WHERE database = $1) AS "referenced!"
