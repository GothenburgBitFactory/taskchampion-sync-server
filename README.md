TaskChampion Sync-Server
------------------------

TaskChampion is the task database [Taskwarrior][tw] uses to store and sync
tasks. This repository implements a sync server against which Taskwarrior
and other applications embedding TaskChampion can sync.

[tw]: https://github.com/GothenburgBitFactory/taskwarrior

## Status

This project provides both pre-built images for common use-cases and Rust
libraries that can be used to build more sophisticated applications. See [the documentation][documentation]
for more on how to use this project.

[documentation]: https://gothenburgbitfactory.org/taskchampion-sync-server

## Repository Guide

The repository is comprised of four crates:

 - `taskchampion-sync-server-core` implements the core of the protocol
 - `taskchampion-sync-server-storage-sqlite` implements an SQLite backend for the core
 - `taskchampion-sync-server-storage-postgres` implements a Postgres backend for the core
 - `taskchampion-sync-server` implements a simple HTTP server for the protocol

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

#### Building TaskChampion Sync-Server

To build TaskChampion Sync-Server binary simply execute the following
commands.
```sh
git clone https://github.com/GothenburgBitFactory/taskchampion-sync-server.git
cd taskchampion-sync-server
cargo build --release
```

After build the binary is located in
`target/release/taskchampion-sync-server`.

#### Building the Postgres Backend

The storage backend is controlled by Cargo features `postres` and `sqlite`.
By default, only the `sqlite` feature is enabled.
To enable building the Postgres backend, add `--features postgres`.
The Postgres binary is located in
`target/release/taskchampion-sync-server-postgres`.

### Building the Docker Images

To build the images, execute the following commands.

SQLite:
```sh
source .env
docker build \
  --build-arg RUST_VERSION=${RUST_VERSION} \
  --build-arg ALPINE_VERSION=${ALPINE_VERSION} \
  -t taskchampion-sync-server docker/sqlite
```

Postgres:
```sh
source .env
docker build \
  --build-arg RUST_VERSION=${RUST_VERSION} \
  --build-arg ALPINE_VERSION=${ALPINE_VERSION} \
  -t taskchampion-sync-server-postgres docker/postgres
```

Now to run it, simply exec.
```sh
docker run -t -d \
  --name=taskchampion \
  -p 8080:8080 \
  taskchampion-sync-server
```

This starts TaskChampion Sync-Server and publishes port 8080 to the host. Please
note that this is a basic run, all data will be destroyed after stop and
delete container. You may also set `DATA_DIR`, `CLIENT_ID`, or `LISTEN` with `-e`, e.g.,

```sh
docker run -t -d \
  --name=taskchampion \
  -e LISTEN=0.0.0.0:9000 \
  -p 9000:9000 \
  taskchampion-sync-server
```
