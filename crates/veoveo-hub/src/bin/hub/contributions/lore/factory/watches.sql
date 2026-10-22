-- veoveo: unbounded (the Hub follows every repository someone turned on, a handful per installation)
-- veoveo: scan (every row is read once when the Hub starts)
SELECT deployment, repository, watcher
FROM hub.lore_watch
