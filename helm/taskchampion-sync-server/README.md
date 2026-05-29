# TaskChampion Sync Server Helm Chart

Deploy the [TaskChampion Sync Server](https://github.com/GothenburgBitFactory/taskchampion-sync-server) on Kubernetes.

## Prerequisites

- Kubernetes 1.23+
- Helm 3+

## Installing

```console
helm repo add taskchampion https://gothenburgbitfactory.org/taskchampion-sync-server
helm repo update
helm install my-release taskchampion/taskchampion-sync-server --set sqlite.enabled=true
```

## Storage Backends

Exactly one storage backend must be enabled. The chart fails validation if neither or both are enabled.

### SQLite

| Parameter | Default | Description |
|-----------|---------|-------------|
| `sqlite.enabled` | `false` | Enable SQLite backend |
| `sqlite.dataDir` | `/var/lib/taskchampion-sync-server/data` | Data directory path |
| `sqlite.existingPV` | `""` | Use an existing PVC name |
| `sqlite.persistence.enabled` | `false` | Create a PVC |
| `sqlite.persistence.size` | `1Gi` | PVC size |
| `sqlite.persistence.accessMode` | `ReadWriteOnce` | PVC access mode |
| `sqlite.emptyDir` | — | emptyDir volume settings |

### PostgreSQL

| Parameter | Default | Description |
|-----------|---------|-------------|
| `postgres.enabled` | `false` | Enable PostgreSQL backend |
| `postgres.host` | `""` | PostgreSQL host |
| `postgres.port` | `5432` | PostgreSQL port |
| `postgres.db` | `taskchampion` | Database name |
| `postgres.username` | `""` | Database user |
| `postgres.password` | `""` | Database password |
| `postgres.sslMode` | `disable` | SSL mode |
| `postgres.existingSecret` | `""` | Use existing secret by name |

**Secret** — When `existingSecret` is empty (default), a secret named `{release-name}-connection` is created with a `conn` key. When set, the chart reads that secret. It accepts either a `conn` key with a full URI or individual keys (`host`, `port`, `username`, `password`, `database`) that override `postgres.*` values.

**Init container** — Enabled by default. Waits for PG readiness, downloads and applies the schema (from the chart's `appVersion` URL), and seeds client IDs if `clientIdSecret` is set. Override with `postgres.initContainer.schemaUrl`.

**Replicas** — Only apply with PostgreSQL (`replicas.enabled=true`, `replicas.count=N`). SQLite is single-replica.

## Secrets

### Client ID Secret

Restrict which client IDs the server accepts. Create a Secret with comma-separated UUIDs (base64-encoded) under a `client-ids` key:

```yaml
apiVersion: v1
kind: Secret
metadata:
  name: my-client-ids
type: Opaque
data:
  client-ids: <base64-encoded comma-separated UUIDs>
```

Reference via `clientIdSecret: "my-client-ids"`.

## Networking

The chart does not implement TLS. Terminate TLS at the ingress or gateway.

### Service

| Parameter | Default | Description |
|-----------|---------|-------------|
| `service.type` | `ClusterIP` | Service type |
| `service.port` | `8080` | Service port |
| `service.targetPort` | `8080` | Container port |

### Ingress (NGINX)

| Parameter | Default | Description |
|-----------|---------|-------------|
| `ingress.enabled` | `false` | Enable NGINX ingress |
| `ingress.className` | `""` | Ingress class name |
| `ingress.annotations` | `{}` | Ingress annotations |
| `ingress.hosts` | `[]` | Host list |
| `ingress.tls` | `[]` | TLS configuration |

### HTTPRoute (Kubernetes Gateway API)

| Parameter | Default | Description |
|-----------|---------|-------------|
| `httpRoute.enabled` | `false` | Enable HTTPRoute |
| `httpRoute.parentRefs` | `[]` | Parent gateway references (primary) |
| `httpRoute.hostnames` | `[]` | Hostnames |
| `httpRoute.rules` | `[]` | Routing rules |
| `httpRoute.gateway` | `""` | (Deprecated) Single gateway name |
| `httpRoute.host` | `""` | (Deprecated) Single hostname |
| `httpRoute.path` | `"/"` | (Deprecated) Single path |
| `httpRoute.port` | `8080` | (Deprecated) Single port |

**Recommended** — use `parentRefs`, `hostnames`, `rules`. The deprecated
single-value fields (`gateway`, `host`, `path`, `port`) are used as a fallback
when the arrays are empty.

## ServiceAccount and RBAC

| Parameter | Default | Description |
|-----------|---------|-------------|
| `serviceAccount.create` | `true` | Create SA, Role, and RoleBinding |
| `serviceAccount.name` | `""` | Use existing SA name |

When created, the chart provisions a ServiceAccount, a Role with
`get`/`create`/`update`/`patch` on secrets, and a RoleBinding.

## Image Configuration

| Parameter | Default | Description |
|-----------|---------|-------------|
| `image.repo` | `ghcr.io/gothenburgbitfactory/taskchampion-sync-server` | Image repository |
| `image.tag` | `"0.7.0"` | Image tag |
| `image.pullPolicy` | `IfNotPresent` | Pull policy |
| `image.pullSecrets` | `[]` | Pull secrets |

PostgreSQL appends `-postgres` to the image repo automatically.

## Environment Variables

Custom env vars are passed via `env`. `DATA_DIR` (SQLite) and `conn`
(PostgreSQL) are set automatically and must not be set manually.

## Full Configuration

See [values.yaml](values.yaml).
