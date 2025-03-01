TaskChampion Sync-Server
------------------------

TaskChampion is the task database [Taskwarrior][tw] uses to store and sync
tasks. This repository implements a sync server against which Taskwarrior
and other applications embedding TaskChampion can sync.

[tw]: https://github.com/GothenburgBitFactory/taskwarrior

## Status

This repository was spun off from Taskwarrior itself after the 3.0.0
release. It is still under development and currently best described as
a reference implementation of the Taskchampion sync protocol.

It is comprised of three crates:

 - `taskchampion-sync-server-core` implements the core of the protocol
 - `taskchmpaion-sync-server-sqlite` implements an SQLite backend for the core
 - `taskchampion-sync-server` implements a simple HTTP server for the protocol

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
[`docker-compose.yml`](https://raw.githubusercontent.com/GothenburgBitFactory/taskchampion-sync-server/refs/tags/v0.6.0/docker-compose.yml)
file in this repository is sufficient to run taskchampion-sync-server,
including setting up TLS certificates using Lets Encrypt, thanks to
[Caddy](https://caddyserver.com/).

You will need a server with ports 80 and 443 open to the Internet and with a
fixed, publicly-resolvable hostname. These ports must be available both to your
Taskwarrior clients and to the Lets Encrypt servers.

On that server, download `docker-compose.yml` from the link above (it is pinned
to the latest release) into the current directory. Then run

```sh
TASKCHAMPION_SYNC_SERVER_HOSTNAME=taskwarrior.example.com docker compose up
```

It can take a few minutes to obtain the certificate; the caddy container will
log a message "certificate obtained successfully" when this is complete, or
error messages if the process fails. Once this process is complete, configure
your `.taskrc`'s to point to the server:

```
sync.server.url=https://taskwarrior.example.com
sync.server.client_id=[your client-id]
sync.encryption_secret=[your encryption secret]
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

By default, the server allows all client IDs. To limit the accepted client IDs,
specify them in the environment variable `CLIENT_ID`, as a comma-separated list
of UUIDs. Client IDs can be specified with `--allow-client-id`, but this should
not be used on shared systems, as command line arguments are visible to all
users on the system.

The server only logs errors by default. To add additional logging output, set
environment variable `RUST_LOG` to `info` to get a log message for every
request, or to `debug` to get more verbose debugging output.

## Building

### Building From Source

#### Installing Rust

TaskChampion Sync-Server build has been tested with current Rust stable
release version. You can install Rust from your distribution package or use
[`rustup`][rustup].
```sh
rustup default stable
```

The minimum supported Rust version (MSRV) is given in
[`Cargo.toml`](./Cargo.toml). Note that package repositories typically do not
have sufficiently new versions of Rust.

If you prefer, you can use the stable version only for installing TaskChampion
Sync-Server (you must clone the repository first).
```sh
rustup override set stable
```

[rustup]: https://rustup.rs/

#### Installing TaskChampion Sync-Server

To build TaskChampion Sync-Server binary simply execute the following
commands.
```sh
git clone https://github.com/GothenburgBitFactory/taskchampion-sync-server.git
cd taskchampion-sync-server
cargo build --release
```

After build the binary is located in
`target/release/taskchampion-sync-server`.

### Building the Container

To build the container execute the following commands.
```sh
source .env
docker build \
  --build-arg RUST_VERSION=${RUST_VERSION} \
  --build-arg ALPINE_VERSION=${ALPINE_VERSION} \
  -t taskchampion-sync-server .
```

Now to run it, simply exec.
```sh
docker run -t -d \
  --name=taskchampion \
  -p 8080:8080 \
  taskchampion-sync-server
```

This start TaskChampion Sync-Server and publish the port to host. Please
note that this is a basic run, all data will be destroyed after stop and
delete container.
