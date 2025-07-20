# Rust Crates

This project publishes several Rust crates on `crates.io`:

- [`taskchampion-sync-server-core`](https://docs.rs/taskchampion-sync-server-core)
implements the core of the protocol
- [`taskchampion-sync-server-storage-sqlite`](https://docs.rs/taskchampion-sync-server-storage-sqlite)
implements an SQLite backend for the core
- [`taskchampion-sync-server-storage-postgres`](https://docs.rs/taskchampion-sync-server-storage-postgres)
implements a Postgres backend for the core

If you are building an integration with, for example, a custom storage system,
it may be helpful to use the `core` crate and provide a custom implementation
of its `Storage` trait.

We suggest that any generally useful extensions, such as additional storage
backends, be published as open-source packages.
