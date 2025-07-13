# Usage

## Running the Server

The server is a simple binary that serves HTTP requests on a TCP port. The
server does not implement TLS; for public deployments, the recommendation is to
use a reverse proxy such as Nginx, haproxy, or Apache httpd.

### Using Docker-Compose

Every release of the server generates a Docker image in
`ghcr.io/gothenburgbitfactory/taskchampion-sync-server`. The tags include
`latest` for the latest release, and both minor and patch versions, e.g., `0.5`
and `0.5.1`.

The
[`docker-compose.yml`](https://raw.githubusercontent.com/GothenburgBitFactory/taskchampion-sync-server/refs/tags/v0.6.1/docker-compose.yml)
file in this repository is sufficient to run taskchampion-sync-server,
including setting up TLS certificates using Lets Encrypt, thanks to
[Caddy](https://caddyserver.com/).

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
ID; omit it to allow all client IDs.

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
typical Docker installation. The docker containers will start automatically on
system startup. See the docker-compose documentation for more information.

### Running the Binary

The server is configured with command-line options. See
`taskchampion-sync-server --help` for full details.

The `--listen` option specifies the interface and port the server listens on.
It must contain an IP-Address or a DNS name and a port number. This option is
mandatory, but can be repeated to specify multiple interfaces or ports. This
value can be specified in environment variable `LISTEN`, as a comma-separated
list of values.

The `--data-dir` option specifies where the server should store its data. This
value can be specified in the environment variable `DATA_DIR`.

By default, the server will allow all clients and create them in the database
on first contact. There are two ways to limit the clients the server will
interact with:

- To limit the accepted client IDs, specify them in the environment variable
`CLIENT_ID`, as a comma-separated list of UUIDs. Client IDs can be specified
with `--allow-client-id`, but this should not be used on shared systems, as
command line arguments are visible to all users on the system. This convenient
option is suitable for personal and small-scale deployments.

- To disable the automatic creation of clients, use the `--no-create-clients`
flag or the `CREATE_CLIENTS=false` environment variable. You are now
responsible for creating clients in the database manually, so this option is
more suitable for large scale deployments.

The server only logs errors by default. To add additional logging output, set
environment variable `RUST_LOG` to `info` to get a log message for every
request, or to `debug` to get more verbose debugging output.

