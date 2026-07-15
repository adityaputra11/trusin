#!/usr/bin/env bash
set -euo pipefail

output=$(printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}' \
  '{"jsonrpc":"2.0","id":3,"method":"resources/list","params":{}}' \
  | env -u TERUSIN_TOKEN cargo run --quiet --bin mcp)

printf '%s\n' "$output" | grep -q '"protocolVersion":"2024-11-05"'
printf '%s\n' "$output" | grep -q '"name":"send_webhook"'
printf '%s\n' "$output" | grep -q '"uri":"trusin://health"'

# Notifications must not produce a JSON-RPC response.
test "$(printf '%s\n' "$output" | wc -l | tr -d ' ')" = "3"
