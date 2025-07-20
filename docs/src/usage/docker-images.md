# Docker Images

Every release of the server generates Docker images. One image is produced for
each storage backend:
- `ghcr.io/gothenburgbitfactory/taskchampion-sync-server` (SQLite)
- `ghcr.io/gothenburgbitfactory/taskchampion-sync-server-postgres` (Postgres)

The image tags include `latest` for the latest release, and both minor and
patch versions, e.g., `0.5` and `0.5.1`.

## Running the Image

At startup, each image applies some default values and runs the relevant binary
directly. Configuration is typically by environment variables, all of which are
documented in the `--help` output of the binaries. These include

- `RUST_LOG` - log level, one of `trace`, `debug`, `info`, `warn` and `error`.
- `DATA_DIR` (SQLite only; default `/var/lib/taskchampion-sync-server/data`) -
directory for the synced data.
- `CONNECTION` (Postgres only) - Postgres connection information, in the form
of a [LibPQ-style connection
URI](https://www.postgresql.org/docs/current/libpq-connect.html#LIBPQ-CONNSTRING-URIS).
- `LISTEN` (default `0.0.0.0:8080`) - address and port on which to listen for
HTTP requests.
- `CLIENT_ID` - comma-separated list of client IDs that will be allowed, or
empty to allow all clients>
- `CREATE_CLIENTS` (default `true`) - if true, automatically create clients on
first sync. If this is set to false, it is up to you to initialize clients in
the DB.

The SQLite image is configured with `VOLUME
/var/lib/taskchampion-sync-server/data`, persisting the task data in an
anonymous Docker volume. It is recommended to put this on a named volume, or
persistent storage in an environment like Kubernetes, so that it is not
accidentally deleted.

The Postgres image does not automatically create its database schema. See the
[integration section](../integration/pre-built.md) for more detail. This
implementation is tested with Postgres version 17 but should work with any
recent version.

Note that the Docker images do not implement TLS. The expectation is that
another component, such as a Kubernetes ingress, will terminate the TLS
connection and proxy HTTP traffic to the taskchampion-sync-server container.
