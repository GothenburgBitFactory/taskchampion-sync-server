# TaskChampion Sync Server Helm Chart

Deploy the [TaskChampion Sync Server](https://github.com/GothenburgBitFactory/taskchampion-sync-server) on Kubernetes.

## Prerequisites

- Kubernetes 1.23+
- Helm 3+

## Storage Backends

Exactly one storage backend must be enabled. The chart will fail validation if both or neither are enabled.

### SQLite

```console
helm install my-release ./helm/taskchampion-sync-server -f helm/taskchampion-sync-server/examples/sqlite-values.yaml
```

### PostgreSQL

```console
helm install my-release ./helm/taskchampion-sync-server -f helm/taskchampion-sync-server/examples/postgres-values.yaml
```

## Secrets

The chart expects pre-created secrets referenced by name:

### Client ID Secret

```yaml
apiVersion: v1
kind: Secret
metadata:
  name: my-client-ids
type: Opaque
data:
  client-ids: <base64-encoded comma-separated UUIDs>
```

Reference it via `clientIdSecret: "my-client-ids"`.

### PostgreSQL Secret

```yaml
apiVersion: v1
kind: Secret
metadata:
  name: my-postgres-creds
type: Opaque
data:
  connection: <base64-encoded LibPQ connection URI>
  password: <base64-encoded password>
```

Reference it via `postgres.existingSecret: "my-postgres-creds"`.

## Configuration

See [values.yaml](values.yaml) for all configurable options.
