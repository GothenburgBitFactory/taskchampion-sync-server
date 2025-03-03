#!/bin/sh
set -e
echo "starting entrypoint script..."
if [ "$1" = "/bin/taskchampion-sync-server" ]; then
    echo "setting data directories"
    mkdir -p "${DATA_DIR}"
    chown -R taskchampion:users "${DATA_DIR}"
    chmod -R 700 "${DATA_DIR}"
    if [ "$(id -u)" = "0" ]; then
        echo "switching to user 'taskchampion'"
        exec su-exec taskchampion "$@"
    fi
fi
