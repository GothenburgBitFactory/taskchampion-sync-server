# Binaries

Taskchampion-sync-server is a single binary that serves HTTP requests on a TCP
port. The server does not implement TLS; for public deployments, the
recommendation is to use a reverse proxy such as Nginx, haproxy, or Apache
httpd.

One binary is provided for each storage backend:

- `taskchampion-sync-server` (SQLite)
- `taskchampion-sync-server-postgres` (Postgres)

### Running the Binary

The server is configured with command-line options or environment variables.
See the `--help` output for full details.

For the SQLite binary, the `--data-dir` option or `DATA_DIR` environment
variable specifies where the server should store its data. For the Postgres
binary, the `--connection` option or `CONNECTION` environment variable
specifies the connection information, in the form of a [LibPQ-style connection
URI](https://www.postgresql.org/docs/current/libpq-connect.html#LIBPQ-CONNSTRING-URIS).
The remaining options are common to all binaries.

The `--listen` option specifies the interface and port the server listens on.
It must contain an IP-Address or a DNS name and a port number. This option is
mandatory, but can be repeated to specify multiple interfaces or ports. This
value can be specified in environment variable `LISTEN`, as a comma-separated
list of values.

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
more suitable for large scale deployments. See [Integration](../integration.md)
for more information on such deployments.

The server only logs errors by default. To add additional logging output, set
environment variable `RUST_LOG` to `info` to get a log message for every
request, or to `debug` to get more verbose debugging output.
