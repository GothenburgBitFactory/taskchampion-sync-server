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

For PostgreSQL, the chart can automatically create a secret with the connection string, or use an existing secret. 

**Automatic Secret Creation**:
- When `postgres.existingSecret` is empty (default), the chart automatically creates a secret
- Secret is named using Helm naming convention: `release-name-taskchampion-sync-server`
- Secret contains only a `connection` key with the built connection string

**Existing Secret Usage**:
- When `postgres.existingSecret` is provided, the chart uses that secret
- The secret **must** contain a `connection` key with the PostgreSQL connection string
- If the secret doesn't have a `connection` key, the deployment will fail with a clear error

## Configuration

See [values.yaml](values.yaml) for all configurable options.
