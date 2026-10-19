UPDATE hub.device_authorization SET poll_interval=$2, next_poll=$3 WHERE id=$1
