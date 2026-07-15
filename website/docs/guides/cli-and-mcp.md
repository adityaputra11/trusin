# CLI and MCP

## CLI

```bash
cargo build --release --bin terusin
./target/release/terusin set-token ts_your_token
./target/release/terusin status
./target/release/terusin events -l 10
./target/release/terusin forward --port 3000
./target/release/terusin interactive
```

`forward` points the default target at a local service and can start ngrok when the backend is remote. Tokens are stored in the OS keychain when available.

## Interactive TUI

`terusin interactive` opens a full-screen terminal dashboard for operators:

- **Overview**: concise health, queue depth, success rate, backend, and auth mode.
- **Events**: recent events, local search with `/`, details with `Enter`, retry with `x`.
- **Rules**: active and inactive routing rules.
- **Config**: backend, dashboard URL, and default target.
- **Tokens**: API token guidance and auth precedence.

Primary shortcuts: `1-5` changes tabs, `r` refreshes, `/` searches, `c` clears search, `o` opens the dashboard, and `q` quits.

## MCP server

Build the binary and configure the AI client with environment variables instead of placing tokens in command arguments.

```json
{
  "mcpServers": {
    "terusin": {
      "command": "/absolute/path/to/target/release/mcp",
      "env": {
        "TERUSIN_URL": "https://your-terusin.example",
        "TERUSIN_TOKEN": "ts_your_token"
      }
    }
  }
}
```
