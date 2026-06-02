#!/usr/bin/env bash
#
# Reproduce the taskchampion-sync-server Postgres connection-pool transaction
# leak end to end, with zero manual setup.
#
# It spins up a throwaway Postgres container, points the repro tests at it, and
# runs them with logging turned up so the diagnostic
#   "WARNING: there is already a transaction in progress"
# is visible in the output.
#
# On the current (unpatched) code, both tests FAIL — that is the proof:
#   - dropped_transaction_leaks_into_next_request  (idle-transaction leak)
#   - aborted_transaction_poisons_pool             (permanent pool poisoning)
#
# After the rollback-on-drop fix is applied, both tests PASS.
#
# Usage:
#   ./repro/run-postgres-bug-repro.sh
#
# Requirements: docker (or podman aliased to docker) and a Rust toolchain.

set -euo pipefail

cd "$(dirname "$0")/.."

CONTAINER_NAME="tc-sync-bug-repro-pg"
HOST_PORT="${HOST_PORT:-54399}"
PG_IMAGE="${PG_IMAGE:-postgres:17}"
PG_PASSWORD="repro"
PG_DB="tc_repro"

cleanup() {
    docker rm -f "$CONTAINER_NAME" >/dev/null 2>&1 || true
}
trap cleanup EXIT

echo ">> Starting throwaway Postgres ($PG_IMAGE) on localhost:$HOST_PORT ..."
cleanup
docker run --rm -d \
    --name "$CONTAINER_NAME" \
    -e POSTGRES_PASSWORD="$PG_PASSWORD" \
    -e POSTGRES_DB="$PG_DB" \
    -p "$HOST_PORT:5432" \
    "$PG_IMAGE" >/dev/null

echo -n ">> Waiting for Postgres to accept connections "
for _ in $(seq 1 60); do
    if docker exec "$CONTAINER_NAME" pg_isready -U postgres >/dev/null 2>&1; then
        echo "- ready."
        break
    fi
    echo -n "."
    sleep 0.5
done

export TEST_DB_URL="host=localhost port=$HOST_PORT user=postgres password=$PG_PASSWORD dbname=$PG_DB"
# Surface Postgres notices/warnings emitted by tokio_postgres.
export RUST_LOG="${RUST_LOG:-info,tokio_postgres=debug}"

echo ">> TEST_DB_URL=$TEST_DB_URL"
echo ">> Running reproduction tests (failures below are the proof of the bug) ..."
echo

set +e
cargo test -p taskchampion-sync-server-storage-postgres \
    -- transaction --nocapture --test-threads=1
status=$?
set -e

echo
if [ "$status" -eq 0 ]; then
    echo ">> Tests PASSED — the pool is no longer leaking transactions (fix is in place)."
else
    echo ">> Tests FAILED — the transaction leak / pool poisoning is reproduced."
fi

exit "$status"
