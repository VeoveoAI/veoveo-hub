SELECT document AS "document: DefinitionDocument"
FROM hub.environment_definition_source
WHERE environment=$1 AND owner_tenant=$2 AND owner_principal=$3
    AND document->'mode'->>'kind'='lore'
