#!/bin/bash
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' | cargo run --bin mcp 2>/dev/null
