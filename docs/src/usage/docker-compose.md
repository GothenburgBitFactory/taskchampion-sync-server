# Docker Compose

The
[`docker-compose.yml`](https://raw.githubusercontent.com/GothenburgBitFactory/taskchampion-sync-server/refs/tags/v0.7.0/docker-compose.yml)
file in this repository is sufficient to run taskchampion-sync-server,
including setting up TLS certificates using Lets Encrypt, thanks to
[Caddy](https://caddyserver.com/). This setup uses the SQLite backend, which is
adequate for one or a few clients.

You will need a server with ports 80 and 443 open to the Internet and with a
fixed, publicly-resolvable hostname. These ports must be available both to your
Taskwarrior clients and to the Lets Encrypt servers.

On that server, download `docker-compose.yml` from the link above (it is pinned
to the latest release) into the current directory. Then run

```sh
TASKCHAMPION_SYNC_SERVER_HOSTNAME=taskwarrior.example.com \
TASKCHAMPION_SYNC_SERVER_CLIENT_ID=your-client-id \
docker compose up
```

The `TASKCHAMPION_SYNC_SERVER_CLIENT_ID` limits the server to the given client
ID; omit it to allow all client IDs. You may specify multiple client IDs
separated by commas.

It can take a few minutes to obtain the certificate; the caddy container will
log a message "certificate obtained successfully" when this is complete, or
error messages if the process fails. Once this process is complete, configure
your `.taskrc`'s to point to the server:

```none
sync.server.url=https://taskwarrior.example.com
sync.server.client_id=your-client-id
sync.encryption_secret=your-encryption-secret
```

The docker-compose images store data in a docker volume named
`taskchampion-sync-server_data`. This volume contains all of the task data, as
well as the TLS certificate information. It will persist over restarts, in a
typical Docker installation. The docker containers will start automatically
when the Docker dameon starts. See the docker-compose documentation for more
information.
