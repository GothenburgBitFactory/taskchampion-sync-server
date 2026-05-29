# Helm

A Helm chart (current version: **0.1.2**, app version: **0.7.0**) is available
for deploying taskchampion-sync-server on Kubernetes.

## Adding the Repository

```sh
helm repo add taskchampion https://gothenburgbitfactory.org/taskchampion-sync-server
helm repo update
```

## Installing

```sh
# SQLite backend
helm install taskchampion-sync-server taskchampion/taskchampion-sync-server \
  --set sqlite.enabled=true

# PostgreSQL backend
helm install taskchampion-sync-server taskchampion/taskchampion-sync-server \
  --set postgres.enabled=true \
  --set postgres.host=my-postgres \
  --set postgres.db=taskchampion \
  --set postgres.username=myuser \
  --set postgres.password=mypassword
```

## Storage Backends

Exactly one storage backend must be enabled. The chart fails validation if
neither or both are enabled.

### SQLite

Mounts a volume at `sqlite.dataDir` (default `/var/lib/taskchampion-sync-server/data`).
The volume defaults to `emptyDir` but can be backed by a PVC or
existing PersistentVolumeClaim:

| Value | Description |
|-------|-------------|
| `sqlite.persistence.enabled` | Create a PVC (default: `false`) |
| `sqlite.existingPV` | Use an existing PVC by name |
| `sqlite.emptyDir` | emptyDir volume settings (default fallback) |

### PostgreSQL

Creates a Deployment with optional replicas, an auto-generated or existing
secret for the connection string, and an init container that waits for
PostgreSQL, applies the schema, and optionally seeds client IDs.

**Secret handling** — When `postgres.existingSecret` is empty (default), the
chart creates a secret named `{release-name}-connection` with a `conn` key.
When set, the chart reads the named secret. It accepts either a `conn` key
with a full [LibPQ-style URI](https://www.postgresql.org/docs/current/libpq-connect.html#LIBPQ-CONNSTRING-URIS)
or individual fields (`host`, `port`, `username`, `password`, `database`) that
override the corresponding `postgres.*` values.

**Init container** — Enabled by default. Waits for PG readiness, downloads
the schema from `https://raw.githubusercontent.com/GothenburgBitFactory/taskchampion-sync-server/v0.7.0/postgres/schema.sql`,
applies it, and seeds client IDs if `clientIdSecret` is set. Override the
schema URL with `postgres.initContainer.schemaUrl`.

**Replicas** — Replicas only apply with PostgreSQL:
`replicas.enabled=true`, `replicas.count=N`. SQLite is always single-replica.

## Secrets

### Client ID Secret

Optional. Restrict which client IDs the server accepts. Create a Secret with
comma-separated UUIDs (base64-encoded) under a `client-ids` key:

```yaml
apiVersion: v1
kind: Secret
metadata:
  name: my-client-ids
type: Opaque
data:
  client-ids: <base64-encoded comma-separated UUIDs>
```

Reference it via `clientIdSecret: "my-client-ids"`. The value is available to
the server as `CLIENT_ID` and to the init container as `CLIENT_IDS`.

## Networking

The chart does not implement TLS. Terminate TLS at the ingress or gateway and
proxy HTTP to port 8080.

### Ingress (NGINX)

```yaml
ingress:
  enabled: true
  hosts:
    - taskchampion.example.com
```

### HTTPRoute (Kubernetes Gateway API)

Use `parentRefs`, `hostnames`, and `rules`:

```yaml
httpRoute:
  enabled: true
  parentRefs:
    - name: my-gateway
  hostnames:
    - tasks.example.com
  rules:
    - path:
        type: PathPrefix
        value: /
      backendPort: 8080
```

The deprecated fields `httpRoute.gateway`, `httpRoute.host`,
`httpRoute.path`, and `httpRoute.port` are used as a fallback when the
structured arrays are empty.

### ServiceAccount and RBAC

When `serviceAccount.create` is `true` (default), the chart creates a
ServiceAccount, a Role with `get`/`create`/`update`/`patch` on secrets, and
a RoleBinding. Set `serviceAccount.name` to use an existing SA.

## Reference

See the chart's [values.yaml](https://github.com/GothenburgBitFactory/taskchampion-sync-server/blob/main/helm/taskchampion-sync-server/values.yaml)
for all configurable options.
