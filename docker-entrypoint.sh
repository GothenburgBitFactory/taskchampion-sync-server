#!/bin/sh
set -e
echo "starting entrypoint script..."
if [ "$1" = "/bin/taskchampion-sync-server" ]; then
    echo "setting data directories"
    mkdir -p "/var/lib/taskchampion-sync-server/data"
    chown -R 100:100 "/var/lib/taskchampion-sync-server/data"
    chmod -R 700 "/var/lib/taskchampion-sync-server/data"
    if [ "$(id -u)" = "0" ]; then
        echo "switching to user 'taskchampion'"
        exec su-exec taskchampion "$@"
    fi
fi
