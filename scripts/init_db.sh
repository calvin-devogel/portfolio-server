#!/usr/bin/env bash
# prints all executed commands to the terminal
set -x
# -e exit if any command has a nonzero exit status
# -o pipefail prevents errors from being masked by
# subsequent pipeline steps
set -eo pipefail

# `>&2`: 
# `>` redirects to std output
# `&` what comes next is a file descriptor, not a file
# `2` stderr file descriptor number
# so: `>&2` redirects stdout from echo to stderr
if ! [ -x "$(command -v sqlx)"]; then
    echo >&2 "Error: sqlx is not installed."
    echo >&2 "Use:"
    echo >&2 "  cargo install sqlx-cli --no-default-features --features rustls, postgres"
    echo >&2 "to install it."
    exit 1
fi

# check if a custom parameter has been set, otherwise use default values
DB_PORT="${POSTGRES_PORT:=5432}"
SUPERUSER="${SUPERUSER:=postgres}"
SUPERUSER_PWD="${SUPERUSER_PWD:=password}"
APP_USER="${APP_USER:=app}"
APP_USER_PWD="${APP_USER_PWD:=secret}"
APP_DB_NAME="${APP_DB_NAME:=portfolio}"

# allow script to skip docker if a containerized instance of postgres is already running
# `-z`: if the length of "${SKIP_DOCKER}" is zero, then do what's in this block, otherwise skip it
if [[ -z "${SKIP_DOCKER}" ]]
then
    # Launch postgres container
    CONTAINER_NAME="postgres"
    docker run \
        --env POSTGRES_USER=${SUPERUSER} \
        --env POSTGRES_PASSWORD=${SUPERUSER_PWD} \
        --health-cmd="pg_isready -U ${SUPERUSER} || exit 1" \
        --health-interval=1s \
        --health-timeout=5s \
        --health-retries=5 \
        --publish "${DB_PORT}":5432 \
        --detach \
        --name "${CONTAINER_NAME}" \
        postgres -N 1000
      # ^ increase the maximum number of connections to 1000
    
    # wait for postgres to be ready to accept connections
    until [ \
        "$(docker inspect -f "{{.State.Health.Status}}" ${CONTAINER_NAME})" == \
        "healthy" \
    ]; do
        >&2 echo "Postgres is still unavailable - sleeping..."
        sleep 1
    done

    # create the application user
    CREATE_QUERY="CREATE USER ${APP_USER} WITH PASSWORD '${APP_USER_PWD}';"
    docker exec -it "${CONTAINER_NAME}" psql -U "${SUPERUSER}" -c "${CREATE_QUERY}"

    # give the app user createdb privileges
    GRANT_QUERY="ALTER USER ${APP_USER} CREATEDB;"
    docker exec -it "${CONTAINER_NAME}" psql -U "${SUPERUSER}" -c "${GRANT_QUERY}"
fi

>&2 echo "Postgres is up and running on port ${DB_PORT} - running migrations now..."

# run migrations
DATABASE_URL=postgres://${APP_USER}:${APP_USER_PWD}@localhost:${DB_PORT}/${APP_DB_NAME}
export DATABASE_URL
sqlx database create
sqlx migrate run

>&2 echo "Postgres has been migrated, ready to go!"
    