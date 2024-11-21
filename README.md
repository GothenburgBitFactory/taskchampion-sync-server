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

The server is configured with command-line options. See
`taskchampion-sync-server --help` for full details.

The `--data-dir` option specifies where the server should store its data, and
`--port` gives the port on which the HTTP server runs. The server does not
implement TLS; for public deployments, the recommendation is to use a reverse
proxy such as Nginx, haproxy, or Apache httpd.

By default, the server allows all client IDs. To limit the accepted client IDs,
such as when running a personal server, use `--allow-client-id <client-id>`.

## Installation

### As container

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

#### Persist data using a container volume

TaskChampion Sync-Server container image uses a volume
`/var/lib/taskchampion-sync-server` to store database. You can exec the
following to mount it in your host as persistent storage.
```sh
docker run -t -d \
  --name=taskchampion \
  -p 8080:8080 \
  -v /my/taskchampion-sync-server:/var/lib/taskchampion-sync-server \
  taskchampion-sync-server
```

Take note that you must create before the directory
`/my/taskchampion-sync-server` and set ownership to UID/GID 100.
```sh
mkdir -p /my/taskchampion-sync-server
chown -R 100:100 /my/taskchampion-sync-server
```

### From source

#### Installing Rust

TaskChampion Sync-Server build has been tested with current Rust stable
release version. You can install Rust from your distribution package or use
[`rustup`][rustup].
```sh
rustup default stable
```

If you prefer, you can use the stable version only for install TaskChampion
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
