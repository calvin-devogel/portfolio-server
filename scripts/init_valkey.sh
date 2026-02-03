#!/usr/bin/env bash
set -x
set -eo pipefail

# if a valkey container is running, print instructions to kill it and exit
RUNNING_CONTAINER=$(docker ps --filter 'name=valkey' --format '{{.ID}}')
# -n is true if the length of $RUNNING_CONTAINER is non-zero (opposite of -z)
if [[ -n $RUNNING_CONTAINER ]]; then
    echo >&2 "There is a valkey container already running, kill it with:"
    echo >&2 "  docker kill ${RUNNING_CONTAINER}"
    exit 1
fi

# launch valkey using docker
# -d == --detach (run in the background)
docker run \
    -p "6379:6379" \
    -d \
    --name "valkey_$(date '+%s')" \
    valkey/valkey:8-alpine

>&2 echo "Valkey is ready to go!"