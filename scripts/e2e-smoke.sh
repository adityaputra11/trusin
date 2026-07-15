#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROJECT=terusin-e2e
COMPOSE=(docker compose -p "$PROJECT" -f "$ROOT/docker-compose.e2e.yml")
PIDS=()

cleanup() {
  for pid in "${PIDS[@]:-}"; do
    kill "$pid" 2>/dev/null || true
  done
  for pid in "${PIDS[@]:-}"; do
    wait "$pid" 2>/dev/null || true
  done
  "${COMPOSE[@]}" down -v --remove-orphans >/dev/null 2>&1 || true
}
trap cleanup EXIT

wait_for() {
  local url="$1"
  for _ in {1..60}; do
    if curl --silent --fail "$url" >/dev/null 2>&1; then return 0; fi
    sleep 1
  done
  echo "Timed out waiting for $url" >&2
  return 1
}

cd "$ROOT"
"${COMPOSE[@]}" up -d --wait

PORT=3330 cargo run --quiet --bin receiver > /tmp/terusin-e2e-receiver.log 2>&1 &
PIDS+=("$!")

DATABASE_URL=postgres://postgres:terusin@127.0.0.1:55432/terusin \
REDIS_URL=redis://127.0.0.1:56379 \
AUTH_USERNAME=admin AUTH_PASSWORD=e2e-password \
DEFAULT_TARGET_URL=http://127.0.0.1:3330/webhook \
PORT=3311 cargo run --quiet --bin backend > /tmp/terusin-e2e-backend.log 2>&1 &
PIDS+=("$!")

AUTH_USERNAME=admin AUTH_PASSWORD=e2e-password \
BACKEND_URL=http://127.0.0.1:3311 PORT=3312 \
cargo run --quiet --bin web > /tmp/terusin-e2e-web.log 2>&1 &
PIDS+=("$!")

wait_for http://127.0.0.1:3311/health
wait_for http://127.0.0.1:3312/health

INDEX="$(curl --silent --fail http://127.0.0.1:3312/)"
grep -q '<div id="root">' <<<"$INDEX"

RESPONSE="$(curl --silent --fail \
  -H 'content-type: application/json' \
  -H 'x-webhook-source: e2e-smoke' \
  -d '{"message":"terusin-e2e"}' \
  http://127.0.0.1:3312/e2e-smoke/webhook)"
EVENT_ID="$(sed -n 's/.*"id":"\([^"]*\)".*/\1/p' <<<"$RESPONSE")"
test -n "$EVENT_ID"

STATUS=""
for _ in {1..30}; do
  EVENT="$(curl --silent --fail -u admin:e2e-password \
    "http://127.0.0.1:3311/events/$EVENT_ID")"
  STATUS="$(sed -n 's/.*"status":"\([^"]*\)".*/\1/p' <<<"$EVENT")"
  if [[ "$STATUS" == "delivered" ]]; then break; fi
  sleep 1
done

if [[ "$STATUS" != "delivered" ]]; then
  echo "Event $EVENT_ID did not reach delivered state (status=$STATUS)" >&2
  exit 1
fi

ATTEMPTS="$(curl --silent --fail -u admin:e2e-password \
  "http://127.0.0.1:3311/events/$EVENT_ID/attempts")"
grep -q '"status":"delivered"' <<<"$ATTEMPTS"
grep -q 'terusin-e2e' /tmp/terusin-e2e-receiver.log

echo "E2E passed: SPA -> web proxy -> backend -> Postgres/Redis -> receiver"
echo "Delivered event: $EVENT_ID"
