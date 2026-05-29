# Helm

A Helm chart is available for deploying taskchampion-sync-server on Kubernetes.

## Adding the Repository

```sh
helm repo add taskchampion https://gothenburgbitfactory.org/taskchampion-sync-server/helm-chart
helm repo update
```

## Installing the Chart

The chart requires exactly one storage backend to be enabled. To install with
the SQLite backend:

```sh
helm install taskchampion-sync-server taskchampion/taskchampion-sync-server \
  --set sqlite.enabled=true
```

To install with the Postgres backend, provide the connection details:

```sh
helm install taskchampion-sync-server taskchampion/taskchampion-sync-server \
  --set postgres.enabled=true \
  --set postgres.host=my-postgres \
  --set postgres.database=taskchampion \
  --set postgres.username=myuser \
  --set postgres.password=mypassword
```

Alternatively, pass an existing Secret name via `postgres.existingSecret`. The
secret must contain a `connection` key holding a [LibPQ-style connection
URI](https://www.postgresql.org/docs/current/libpq-connect.html#LIBPQ-CONNSTRING-URIS).

## Configuration

The chart does not implement TLS. The expectation is that a Kubernetes ingress
or gateway will terminate TLS and proxy HTTP traffic to the server container.
Enable one of the built-in options or configure your own:

```sh
# NGINX ingress
--set ingress.enabled=true --set ingress.hosts[0]=taskchampion.example.com

# Gateway API HTTPRoute
--set httpRoute.enabled=true \
--set httpRoute.gateway=my-gateway \
--set httpRoute.host=taskchampion.example.com
```

To restrict which client IDs the server accepts, create a Secret containing a
newline-separated list of UUIDs and reference it:

```sh
--set clientIdSecret=my-client-ids-secret
```

See the chart's `values.yaml` for the full set of configuration options.
