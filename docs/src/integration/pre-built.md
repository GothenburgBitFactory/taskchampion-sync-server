# Pre-built Images

The pre-built Postgres Docker image described in [Docker
Images](../usage/docker-images.md) may be adequate for a production deployment.
The image is stateless and can be easily scaled horizontally to increase
capacity.

## Database Schema

The schema defined in
[`postgres/schema.sql`](https://github.com/GothenburgBitFactory/taskchampion-sync-server/blob/main/postgres/schema.sql)
must be applied to the database before the container will function.

The schema is stable, and any changes to the schema will be made in a major
version with migration instructions provided.

An integration may:

- Add additional tables to the database
- Add additional columns to the `clients` table. If those columns do not have
default values, ensure the server is configured with `CREATE_CLIENTS=false` as
described below.
- Insert rows into the `clients` table, using default values for all columns
except `client_id` and any application-specific columns.
- Delete rows from the `clients` table, noting that associated task data is
also deleted.

## Managing Clients

By default, taskchampion-sync-server creates a new, empty client when it
receives a connection from an unrecognized client ID. Setting
`CREATE_CLIENTS=false` disables this functionality, and is recommended in
production deployments to avoid abuse.

In this configuration, it is the responsibility of the integration to create
new client rows when desired, using a statement like `INSERT into clients
(client_id) values ($1)` with the new client ID as a parameter. Similarly,
clients may be deleted, along with all stored task data, using a statement like
`DELETE from clients where client_id = $1`.
