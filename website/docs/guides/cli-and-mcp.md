# CLI and MCP

## CLI

### Install the CLI

Install the latest verified release on macOS (Apple Silicon or Intel) or Linux
(x86_64 or ARM64):

```bash
curl -fsSL https://download.trusin.my.id/install.sh | sh
```

The installer downloads the matching GitHub Release asset, verifies its SHA-256
checksum, and installs `trusin` to `/usr/local/bin`. It prompts for `sudo` only
when that directory is not writable.

Install a specific release or use a writable directory without `sudo`:

```bash
curl -fsSL https://download.trusin.my.id/install.sh | TERUSIN_VERSION=v0.1.0 sh
curl -fsSL https://download.trusin.my.id/install.sh | TERUSIN_INSTALL="$HOME/.local/bin" sh
```

After installation, create an API token in **Settings → Developer → API Tokens**
and connect the device:

```bash
trusin set-token ts_your_token
trusin status
```

If the command is not found after a custom installation, add that directory to
your shell `PATH`. The installer supports macOS and Linux only; Windows users can
build the CLI from source.

### Build from source

```bash
cargo build --release --bin trusin
./target/release/trusin set-token ts_your_token
./target/release/trusin status
./target/release/trusin events -l 10
./target/release/trusin forward --port 3000
./target/release/trusin interactive
```

`forward` points the default target at a local service and can start ngrok when the backend is remote. Tokens are stored in the OS keychain when available.

## Interactive TUI

`trusin interactive` opens a full-screen terminal dashboard for operators:

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
    "trusin": {
      "command": "/absolute/path/to/target/release/mcp",
      "env": {
        "TERUSIN_URL": "https://your-terusin.example",
        "TERUSIN_TOKEN": "ts_your_token"
      }
    }
  }
}
```
