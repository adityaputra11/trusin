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

## Uninstall

Remove the CLI, its bundled MCP server, and local credentials with a confirmation prompt:

```sh
trusin uninstall
```

For scripts, pass `--yes`. This does not delete your workspace or revoke API tokens; manage
those from Settings in the dashboard.

After installation, create an API token in **Settings → Developer → API Tokens**
and run the CLI:

```bash
trusin
```

On its first run, `trusin` asks for the `ts_...` token, stores it in the OS
keychain when available, and opens the interactive TUI. The CLI does not use
your dashboard password. To add or replace a token non-interactively, run
`trusin set-token ts_your_token`.

If the command is not found after a custom installation, add that directory to
your shell `PATH`. The installer supports macOS and Linux only; Windows users can
build the CLI from source.

### Build from source

```bash
cargo build --release --bin trusin
./target/release/trusin
./target/release/trusin status
./target/release/trusin events -l 10
./target/release/trusin forward --port 3000
./target/release/trusin interactive
```

`forward` points the default target at a local service and can start ngrok when the backend is remote. Tokens are stored in the OS keychain when available.

## Local development without ngrok

Use `trusin dev` to mirror new events from one source to your local service. The
CLI keeps an authenticated outbound stream to trusin, so your laptop does not
need a public URL and private targets never reach the production API.

Start a local receiver or your application:

```bash
cargo run --bin receiver
```

In another terminal, mirror a source such as Stripe:

```bash
trusin dev --source stripe --port 3000
```

`trusin dev` adds `X-Trusin-Event-Id` and `X-Trusin-Source` headers, then posts
the original JSON body to `http://127.0.0.1:<port>`. It is a **mirror**: normal
production delivery and event status are left unchanged. Use `trusin forward`
only when you explicitly need trusin to make your local server its delivery target.

## Interactive TUI

Running `trusin` opens a full-screen terminal dashboard for operators after
token onboarding. `trusin interactive` remains available when you want to open
the dashboard explicitly:

- **Overview**: concise health, queue depth, success rate, backend, and auth mode.
- **Events**: recent events, local search with `/`, details with `Enter`, retry with `x`.
- **Rules**: active and inactive routing rules.
- **Config**: backend, dashboard URL, and default target.
- **Tokens**: API token guidance and auth precedence.

Primary shortcuts: `1-5` changes tabs, `r` refreshes, `/` searches, `c` clears search, `o` opens the dashboard, and `q` quits.

## MCP server

The installer bundles the MCP sidecar. Save your API token once, then configure
your AI client to launch `trusin mcp`; the CLI passes its saved token and backend
configuration to the stdio server without storing credentials in the client config.

```bash
trusin set-token ts_your_token
```

```json
{
  "mcpServers": {
    "trusin": {
      "command": "trusin",
      "args": ["mcp"]
    }
  }
}
```

### OpenCode

OpenCode uses an `mcp` object rather than `mcpServers`. Add this to
`~/.config/opencode/opencode.json` (or your project `opencode.json`), then restart OpenCode:

```json
{
  "$schema": "https://opencode.ai/config.json",
  "mcp": {
    "trusin": {
      "type": "local",
      "command": ["/usr/local/bin/trusin", "mcp"],
      "enabled": true,
      "timeout": 10000
    }
  }
}
```

Run `trusin set-token ts_...` before launching OpenCode. The wrapper reads the saved
token from your OS keychain, so the token does not need to be stored in OpenCode's config.
Use `opencode mcp list` to confirm that the server is connected.

For a custom sidecar location, set `TRUSIN_MCP_PATH` before launching
`trusin mcp`. Direct executions of `trusin-mcp` continue to accept
`TERUSIN_URL` and `TERUSIN_TOKEN`.
