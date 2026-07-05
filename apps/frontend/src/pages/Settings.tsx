import { useMemo, useState } from "react";
import {
  Activity,
  Copy,
  Check,
  Terminal,
  Plug,
  Server,
  Wrench,
} from "lucide-react";
import { Card, CardHeader, Badge } from "../components/ui";
import { useHealth } from "../lib/hooks";
import { useCurrentUser } from "../lib/user-context";

// The MCP server is a stdio process — its tools are static and known.
const MCP_TOOLS = [
  {
    name: "list_events",
    description: "List recent webhook events",
    args: "limit?: number (default 20)",
  },
  {
    name: "retry_event",
    description: "Retry a failed webhook event",
    args: "id: string (required)",
  },
  {
    name: "send_webhook",
    description: "Send a webhook through the relay",
    args: "target_url, body, source?",
  },
  {
    name: "health",
    description: "Check backend health",
    args: "—",
  },
] as const;

type ClientKey = "claude" | "cursor" | "vscode";

const CLIENTS: { key: ClientKey; label: string; file: string }[] = [
  { key: "claude", label: "Claude Desktop", file: "claude_desktop_config.json" },
  { key: "cursor", label: "Cursor", file: ".cursor/mcp.json" },
  { key: "vscode", label: "VS Code (Copilot)", file: "settings.json" },
];

function buildSnippet(client: ClientKey, binaryPath: string): string {
  const env = {
    TERUSIN_USER: "admin",
    TERUSIN_PASS: "<your-password>",
  };
  const server = { command: binaryPath, env };
  switch (client) {
    case "claude":
      return JSON.stringify({ mcpServers: { terusin: server } }, null, 2);
    case "cursor":
      return JSON.stringify({ mcpServers: { terusin: server } }, null, 2);
    case "vscode":
      // VS Code Copilot Chat uses chat.mcp.discovery.enabled + a servers map
      // under "mcp.servers" (preview). We document the most common shape.
      return JSON.stringify(
        {
          "chat.mcp.discovery.enabled": true,
          "mcp.servers": { terusin: server },
        },
        null,
        2,
      );
  }
}

function CodeBlock({ code }: { code: string }) {
  const [copied, setCopied] = useState(false);
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(code);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      /* ignore */
    }
  };
  return (
    <div className="relative">
      <pre className="bg-surface border border-border rounded-md p-3 text-xs text-secondary font-mono overflow-x-auto pr-20">
        {code}
      </pre>
      <button
        onClick={copy}
        className="absolute top-2 right-2 p-1.5 rounded-md bg-hover text-muted hover:text-foreground transition-base"
        title="Copy"
      >
        {copied ? (
          <Check className="h-3.5 w-3.5 text-success" />
        ) : (
          <Copy className="h-3.5 w-3.5" />
        )}
      </button>
    </div>
  );
}

export function Settings() {
  const health = useHealth();
  const user = useCurrentUser();
  const [client, setClient] = useState<ClientKey>("claude");
  const [binaryPath, setBinaryPath] = useState(
    "/usr/local/bin/terusin-mcp",
  );

  const snippet = useMemo(
    () => buildSnippet(client, binaryPath),
    [client, binaryPath],
  );
  const activeClient = CLIENTS.find((c) => c.key === client)!;

  const healthy = health.data?.status === "ok";

  return (
    <div className="max-w-3xl space-y-6">
      {/* System status */}
      <Card>
        <CardHeader
          title="System Status"
          subtitle="Backend connectivity & your session"
        />
        <div className="grid grid-cols-2 gap-4 text-sm">
          <div className="flex items-center gap-3">
            <div
              className={`h-2.5 w-2.5 rounded-full ${
                healthy ? "bg-success" : health.isError ? "bg-danger" : "bg-warning animate-pulse"
              }`}
            />
            <div>
              <p className="text-foreground font-medium">
                Backend {healthy ? "online" : health.isError ? "offline" : "…"}
              </p>
              <p className="text-xs text-muted">
                {healthy
                  ? "Accepting webhooks & serving API"
                  : "Cannot reach /health"}
              </p>
            </div>
          </div>
          <div className="flex items-center gap-3">
            <Server className="h-4 w-4 text-muted" />
            <div>
              <p className="text-foreground font-medium">{user?.email ?? user?.username ?? "—"}</p>
              <p className="text-xs text-muted">
                {user ? `Signed in via ${user.oauth_provider ?? "password"}` : "Not signed in"}
              </p>
            </div>
          </div>
        </div>
      </Card>

      {/* MCP setup */}
      <Card>
        <CardHeader
          title="MCP Server Setup"
          subtitle="Connect an AI client to your Terusin relay"
          action={<Badge variant="purple">stdio</Badge>}
        />

        <div className="space-y-5">
          <p className="text-sm text-secondary">
            The Terusin MCP server exposes your events & relay actions to AI
            clients (Claude Desktop, Cursor, VS Code Copilot). It runs as a
            local stdio process spawned by the client — no separate port.
          </p>

          {/* Tools list */}
          <div>
            <p className="text-xs font-medium text-secondary uppercase mb-2 flex items-center gap-1.5">
              <Wrench className="h-3.5 w-3.5" /> Available tools
            </p>
            <div className="grid grid-cols-1 sm:grid-cols-2 gap-2">
              {MCP_TOOLS.map((t) => (
                <div
                  key={t.name}
                  className="bg-surface border border-border rounded-md p-3"
                >
                  <code className="text-xs font-mono text-foreground font-semibold">
                    {t.name}
                  </code>
                  <p className="text-xs text-muted mt-1">{t.description}</p>
                  <p className="text-[10px] text-muted mt-1 font-mono">
                    args: {t.args}
                  </p>
                </div>
              ))}
            </div>
          </div>

          {/* Binary path input */}
          <div>
            <label className="text-xs font-medium text-secondary block mb-1.5">
              Path to MCP binary (after{" "}
              <code className="text-foreground">cargo build --release --bin mcp</code>)
            </label>
            <input
              value={binaryPath}
              onChange={(e) => setBinaryPath(e.target.value)}
              className="w-full bg-surface border border-border rounded-md text-foreground px-4 py-2.5 text-sm font-mono focus:outline-none focus:border-success"
              spellCheck={false}
            />
          </div>

          {/* Client selector */}
          <div className="flex gap-1 bg-surface border border-border rounded-md p-1 w-fit">
            {CLIENTS.map((c) => (
              <button
                key={c.key}
                onClick={() => setClient(c.key)}
                className={`px-3 py-1.5 rounded text-xs font-medium transition-base ${
                  client === c.key
                    ? "bg-hover text-foreground"
                    : "text-muted hover:text-foreground"
                }`}
              >
                {c.label}
              </button>
            ))}
          </div>

          {/* Snippet */}
          <div>
            <p className="text-xs font-medium text-secondary mb-1.5 flex items-center gap-1.5">
              <Terminal className="h-3.5 w-3.5" />
              {activeClient.label} —{" "}
              <code className="text-muted">{activeClient.file}</code>
            </p>
            <CodeBlock code={snippet} />
            <p className="text-[11px] text-muted mt-2">
              {client === "claude" &&
                "macOS: ~/Library/Application Support/Claude/claude_desktop_config.json · Linux: ~/.config/Claude/"}
              {client === "cursor" &&
                "Project-level: .cursor/mcp.json in your repo · Global: ~/.cursor/mcp.json"}
              {client === "vscode" &&
                "User settings (Cmd+,). Requires the Copilot Chat MCP preview flag enabled."}
            </p>
          </div>

          {/* Install steps */}
          <div className="bg-surface border border-border rounded-md p-4">
            <p className="text-xs font-semibold text-secondary uppercase mb-2 flex items-center gap-1.5">
              <Plug className="h-3.5 w-3.5" /> Quick start
            </p>
            <ol className="text-sm text-secondary space-y-1.5 list-decimal list-inside">
              <li>
                Build the binary:{" "}
                <code className="text-foreground font-mono text-xs">
                  cargo build --release --bin mcp
                </code>
              </li>
              <li>
                Install it (optional):{" "}
                <code className="text-foreground font-mono text-xs">
                  cp target/release/mcp {binaryPath}
                </code>
              </li>
              <li>Copy the snippet above into your client's config file.</li>
              <li>Restart the client — the AI can now call your tools.</li>
            </ol>
          </div>

          {/* Test */}
          <div className="flex items-center gap-3 text-sm">
            <Activity className="h-4 w-4 text-muted" />
            <span className="text-secondary">Verify the binary works:</span>
            <code className="text-xs font-mono text-foreground bg-surface border border-border rounded px-2 py-1">
              {`echo '{"method":"health"}' | `}{binaryPath}
            </code>
          </div>
        </div>
      </Card>

      {/* Env vars reference */}
      <Card>
        <CardHeader
          title="Environment Variables"
          subtitle="Read by the MCP binary at startup"
        />
        <div className="space-y-2">
          {[
            {
              k: "TERUSIN_USER",
              v: "admin",
              desc: "Basic auth username (must match backend AUTH_USERNAME)",
            },
            {
              k: "TERUSIN_PASS",
              v: "—",
              desc: "Basic auth password (must match backend AUTH_PASSWORD)",
            },
          ].map((row) => (
            <div
              key={row.k}
              className="flex items-start gap-4 py-2 border-b border-border last:border-0"
            >
              <code className="text-xs font-mono text-success w-36 shrink-0">
                {row.k}
              </code>
              <div className="min-w-0">
                <p className="text-sm text-foreground">
                  default: <code className="text-xs font-mono text-muted">{row.v}</code>
                </p>
                <p className="text-xs text-muted">{row.desc}</p>
              </div>
            </div>
          ))}
          <p className="text-xs text-muted pt-2">
            The backend URL is currently hardcoded to{" "}
            <code className="text-foreground font-mono">http://127.0.0.1:3011</code>{" "}
            in <code className="text-foreground font-mono">apps/mcp/src/main.rs</code>.
            Edit and rebuild if your backend runs elsewhere.
          </p>
        </div>
      </Card>
    </div>
  );
}
