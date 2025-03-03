#!/bin/sh
set -e
echo "starting entrypoint script..."
if [ "$1" = "/bin/taskchampion-sync-server" ]; then
    : ${DATA_DIR:=/var/lib/taskchampion-sync-server}
    export DATA_DIR
    echo "setting up data directory ${DATA_DIR}"
    mkdir -p "${DATA_DIR}"
    chown -R taskchampion:users "${DATA_DIR}"
    chmod -R 700 "${DATA_DIR}"

    : ${LISTEN:=0.0.0.0:8080}
    export LISTEN
    echo "Listen set to ${LISTEN}"

    if [ -n "${CLIENT_ID}" ]; then
        export CLIENT_ID
        echo "Limiting to client ID ${CLIENT_ID}"
    else
        unset CLIENT_ID
    fi

    if [ "$(id -u)" = "0" ]; then
        echo "Running server as user 'taskchampion'"
        exec su-exec taskchampion "$@"
    fi
else
    eval "${@}"
fi
