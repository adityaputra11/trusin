# CLI dan MCP

## CLI

```bash
cargo build --release --bin terusin
./target/release/terusin set-token ts_your_token
./target/release/terusin status
./target/release/terusin events -l 10
./target/release/terusin forward --port 3000
./target/release/terusin interactive
```

`forward` mengarahkan default target ke service lokal dan dapat menjalankan ngrok saat backend remote. Token disimpan di OS keychain bila tersedia.

## Interactive TUI

`terusin interactive` membuka terminal dashboard full-screen untuk operator:

- **Overview**: health ringkas, queue depth, success rate, backend, dan auth mode.
- **Events**: event terbaru, search lokal dengan `/`, detail dengan `Enter`, retry dengan `x`.
- **Rules**: daftar routing rule aktif/nonaktif.
- **Config**: backend, dashboard URL, dan default target.
- **Tokens**: panduan API token dan precedence auth.

Shortcut utama: `1-5` pindah tab, `r` refresh, `/` search, `c` clear search, `o` buka dashboard, `q` keluar.

## MCP server

Build binary lalu konfigurasikan AI client dengan environment variable, bukan menaruh token di argument command.

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
