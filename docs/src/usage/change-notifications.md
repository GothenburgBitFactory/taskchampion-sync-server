# Change Notifications

The HTTP server exposes `GET /v1/client/events` as a Server-Sent Events stream.
Like other client endpoints, the request must include `X-Client-Id`.

This endpoint is disabled by default. Enable it with `--sync-events` or the
`SYNC_EVENTS=true` environment variable.

When `AddVersion` accepts a new version for that client, the stream emits a
`version` event:

```text
event: version
data: {"clientId":"..."}
```

This endpoint is only an invalidation signal. Clients should perform a normal
TaskChampion sync after receiving an event.

## Simple Listener

This example runs a command for every received `version` event.

```bash
#!/usr/bin/env bash
set -euo pipefail

server_url="${TASKCHAMPION_SYNC_SERVER_URL:?set TASKCHAMPION_SYNC_SERVER_URL}"
client_id="${TASKCHAMPION_SYNC_CLIENT_ID:?set TASKCHAMPION_SYNC_CLIENT_ID}"

curl -fsSN \
  -H "Accept: text/event-stream" \
  -H "X-Client-Id: ${client_id}" \
  "${server_url%/}/v1/client/events" |
while IFS= read -r line; do
  case "${line}" in
    data:*)
      echo "TaskChampion changed: ${line#data: }"
      task sync
      ;;
  esac
done
```
