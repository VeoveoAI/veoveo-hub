UPDATE hub.activation_head SET generation = generation + 1, serving = NULL
WHERE owner_tenant=$1 AND owner_principal=$2 AND environment=$3 AND component=$4
    AND generation = (SELECT generation FROM hub.activation_build WHERE id = desired)
