import { useMemo, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import {
  Activity,
  Building2,
  Code2,
  Copy,
  Check,
  ChevronRight,
  LockKeyhole,
  Terminal,
  Plug,
  Server,
  Wrench,
  Trash2,
  Plus,
  KeyRound,
  AlertTriangle,
  ShieldCheck,
  Users,
} from "lucide-react";
import { Card, CardHeader, Badge, Button, Input, Modal } from "../components/ui";
import {
  useHealth,
  useTokens,
  useCreateToken,
  useRevokeToken,
} from "../lib/hooks";
import { useCanWrite, useCurrentUser } from "../lib/user-context";
import { formatRelative } from "../lib/format";
import { Activity as ActivityPage } from "./Activity";
import { Organization } from "./Organization";
import { Users as UsersPage } from "./Users";

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
  // Token is the preferred auth mode — generate one in "Devices & API Tokens"
  // above, then paste it here. User/pass is the legacy fallback.
  const env = {
    TERUSIN_TOKEN: "<ts_... from Settings → Devices & Tokens>",
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

/** Devices & API Tokens card. Generate an API key (shown once in a popup) for
 *  the CLI / MCP, and revoke keys later. Each key is scoped to the signed-in
 *  user's role (admin = full, viewer = read-only). */
function DevicesAndTokens() {
  const { data: tokens, isLoading } = useTokens();
  const createToken = useCreateToken();
  const revoke = useRevokeToken();
  const [showForm, setShowForm] = useState(false);
  const [name, setName] = useState("");
  const [created, setCreated] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  const generate = async () => {
    const tokenName = name.trim() || "My device";
    try {
      const res = await createToken.mutateAsync(tokenName);
      setCreated(res.token);
      setName("");
    } catch {
      /* toast/error handled inline by mutation state */
    }
  };

  const closeForm = () => {
    setShowForm(false);
    setCreated(null);
    setName("");
    setCopied(false);
  };

  const copyAndClose = async () => {
    if (!created) return;
    try {
      await navigator.clipboard.writeText(created);
      setCopied(true);
      setTimeout(() => closeForm(), 1200);
    } catch {
      closeForm();
    }
  };

  return (
    <Card>
      <CardHeader
        title="API Tokens"
        subtitle="Generate a key for the CLI / MCP — no password shared"
        action={
          <Button size="sm" onClick={() => setShowForm(true)}>
            <Plus className="h-4 w-4" /> New key
          </Button>
        }
      />

      {/* Generate modal */}
      <Modal
        open={showForm}
        onClose={created ? () => {} : closeForm}
        title={created ? "API Key Generated" : "Generate API Key"}
        description={
          created
            ? "Copy it now — you won't be able to see it again"
            : "Name this key so you can identify it later"
        }
        footer={
          !created ? (
            <>
              <Button variant="ghost" onClick={closeForm}>Cancel</Button>
              <Button onClick={generate} loading={createToken.isPending}>
                Generate
              </Button>
            </>
          ) : (
            <Button onClick={copyAndClose} variant="primary" className="w-full">
              {copied ? (
                <><Check className="h-4 w-4 text-success" /> Copied!</>
              ) : (
                <><Copy className="h-4 w-4" /> Copy &amp; close</>
              )}
            </Button>
          )
        }
      >
        {!created ? (
          <Input
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="e.g. MacBook CLI"
            maxLength={120}
            autoFocus
            onKeyDown={(e) => e.key === "Enter" && generate()}
          />
        ) : (
          <div className="space-y-4">
            <div className="bg-[rgba(234,179,8,.08)] border border-[rgba(234,179,8,.3)] rounded-md p-4">
              <p className="text-xs font-medium text-warning uppercase tracking-wide flex items-center gap-1.5 mb-3">
                <AlertTriangle className="h-3.5 w-3.5" /> Shown only once
              </p>
              <CodeBlock code={created} />
            </div>
            <p className="text-xs text-muted text-center">
              Then run: <code className="text-foreground font-mono">terusin set-token {created.slice(0, 6)}…</code>
            </p>
          </div>
        )}
      </Modal>

      {createToken.isError && (
        <p className="text-sm text-danger bg-[rgba(239,68,68,.1)] border border-[rgba(239,68,68,.25)] rounded-md p-3 mb-4">
          Could not generate a key. Make sure the backend is reachable.
        </p>
      )}

      {/* Active tokens list */}
      <div>
        <p className="text-xs font-medium text-secondary uppercase mb-2">
          Active keys
        </p>
        {isLoading ? (
          <p className="text-sm text-muted">Loading…</p>
        ) : !tokens || tokens.length === 0 ? (
          <p className="text-sm text-muted py-4">
            No API keys yet. Tap "New key" above to generate one.
          </p>
        ) : (
          <div className="space-y-1">
            {tokens.map((t) => (
              <div
                key={t.id}
                className="flex items-center justify-between gap-3 bg-surface border border-border rounded-md p-3"
              >
                <div className="flex items-center gap-3 min-w-0">
                  <KeyRound className="h-4 w-4 text-muted shrink-0" />
                  <div className="min-w-0">
                    <p className="text-sm text-foreground font-medium truncate">
                      {t.name}
                    </p>
                    <p className="text-xs text-muted">
                      {t.last_used_at
                        ? `Used ${formatRelative(t.last_used_at)}`
                        : "Never used"}{" "}
                      · created {formatRelative(t.created_at)}
                    </p>
                    <p className="text-[10px] text-muted mt-1 font-mono truncate">
                      scopes: {t.scopes.join(", ")}
                    </p>
                  </div>
                </div>
                <button
                  onClick={() => revoke.mutate(t.id)}
                  className="p-2 rounded-md text-muted hover:text-danger hover:bg-[rgba(239,68,68,.1)] transition-base shrink-0"
                  title="Revoke"
                >
                  <Trash2 className="h-4 w-4" />
                </button>
              </div>
            ))}
          </div>
        )}
      </div>
    </Card>
  );
}

function GeneralTab() {
  const health = useHealth();
  const user = useCurrentUser();
  const healthy = health.data?.status === "ok";

  return (
    <div className="space-y-6">
      <Card>
        <CardHeader title="System Status" subtitle="Backend & session" />
        <div className="grid grid-cols-2 gap-4 text-sm">
          <div className="flex items-center gap-3">
            <div className={`h-2.5 w-2.5 rounded-full ${healthy ? "bg-success" : health.isError ? "bg-danger" : "bg-warning animate-pulse"}`} />
            <div>
              <p className="text-foreground font-medium">Backend {healthy ? "online" : health.isError ? "offline" : "\u2026"}</p>
              <p className="text-xs text-muted">{healthy ? "Accepting webhooks & serving API" : "Cannot reach /health"}</p>
            </div>
          </div>
          <div className="flex items-center gap-3">
            <Server className="h-4 w-4 text-muted" />
            <div>
              <p className="text-foreground font-medium">{user?.email ?? user?.username ?? "\u2014"}</p>
              <p className="text-xs text-muted">{user ? `Signed in via ${user.oauth_provider ?? "password"}` : "Not signed in"}</p>
            </div>
          </div>
        </div>
      </Card>

      <Card>
        <CardHeader title="Environment Variables" subtitle="Read by the MCP binary at startup" />
        <div className="space-y-2">
          {[
            { k: "TERUSIN_TOKEN", v: "\u2014", desc: "API token (preferred). Bearer auth." },
            { k: "TERUSIN_USER", v: "admin", desc: "Basic auth username (legacy fallback)." },
            { k: "TERUSIN_PASS", v: "\u2014", desc: "Basic auth password (legacy fallback)." },
          ].map((row) => (
            <div key={row.k} className="flex items-start gap-4 py-2 border-b border-border last:border-0">
              <code className="text-xs font-mono text-success w-36 shrink-0">{row.k}</code>
              <div className="min-w-0">
                <p className="text-sm text-foreground">default: <code className="text-xs font-mono text-muted">{row.v}</code></p>
                <p className="text-xs text-muted">{row.desc}</p>
              </div>
            </div>
          ))}
        </div>
      </Card>
    </div>
  );
}

function McpTab() {
  const [client, setClient] = useState<ClientKey>("claude");
  const [binaryPath, setBinaryPath] = useState("/usr/local/bin/terusin-mcp");

  const snippet = useMemo(() => buildSnippet(client, binaryPath), [client, binaryPath]);
  const activeClient = CLIENTS.find((c) => c.key === client)!;

  return (
    <Card>
      <CardHeader title="MCP Server Setup" subtitle="Connect an AI client to your trusin relay" action={<Badge variant="purple">stdio</Badge>} />
      <div className="space-y-5">
        <p className="text-sm text-secondary">
          The trusin MCP server exposes your events & relay actions to AI clients. It runs as a local stdio process spawned by the client.
        </p>

        <div>
          <p className="text-xs font-medium text-secondary uppercase mb-2 flex items-center gap-1.5">
            <Wrench className="h-3.5 w-3.5" /> Available tools
          </p>
          <div className="grid grid-cols-1 sm:grid-cols-2 gap-2">
            {MCP_TOOLS.map((t) => (
              <div key={t.name} className="bg-surface border border-border rounded-md p-3">
                <code className="text-xs font-mono text-foreground font-semibold">{t.name}</code>
                <p className="text-xs text-muted mt-1">{t.description}</p>
                <p className="text-[10px] text-muted mt-1 font-mono">args: {t.args}</p>
              </div>
            ))}
          </div>
        </div>

        <div>
          <label className="text-xs font-medium text-secondary block mb-1.5">
            Path to MCP binary: <code className="text-foreground">cargo build --release --bin mcp</code>
          </label>
          <input value={binaryPath} onChange={(e) => setBinaryPath(e.target.value)} className="w-full bg-surface border border-border rounded-md text-foreground px-4 py-2.5 text-sm font-mono focus:outline-none focus:border-success" spellCheck={false} />
        </div>

        <div className="flex gap-1 bg-surface border border-border rounded-md p-1 w-fit">
          {CLIENTS.map((c) => (
            <button key={c.key} onClick={() => setClient(c.key)} className={`px-3 py-1.5 rounded text-xs font-medium transition-base ${client === c.key ? "bg-hover text-foreground" : "text-muted hover:text-foreground"}`}>{c.label}</button>
          ))}
        </div>

        <div>
          <p className="text-xs font-medium text-secondary mb-1.5 flex items-center gap-1.5">
            <Terminal className="h-3.5 w-3.5" /> {activeClient.label} — <code className="text-muted">{activeClient.file}</code>
          </p>
          <CodeBlock code={snippet} />
        </div>

        <div className="bg-surface border border-border rounded-md p-4">
          <p className="text-xs font-semibold text-secondary uppercase mb-2 flex items-center gap-1.5"><Plug className="h-3.5 w-3.5" /> Quick start</p>
          <ol className="text-sm text-secondary space-y-1.5 list-decimal list-inside">
            <li>Build binary: <code className="text-foreground font-mono text-xs">cargo build --release --bin mcp</code></li>
            <li>Install: <code className="text-foreground font-mono text-xs">cp target/release/mcp {binaryPath}</code></li>
            <li>Copy snippet into your client's config file.</li>
            <li>Restart the AI client.</li>
          </ol>
        </div>

        <div className="flex items-center gap-3 text-sm">
          <Activity className="h-4 w-4 text-muted" />
          <span className="text-secondary">Verify:</span>
          <code className="text-xs font-mono text-foreground bg-surface border border-border rounded px-2 py-1">{`echo '{"method":"health"}' | `}{binaryPath}</code>
        </div>
      </div>
    </Card>
  );
}

export function Settings() {
  const navigate = useNavigate();
  const { section } = useParams();
  const canWrite = useCanWrite();
  const activeSection = isSettingsSection(section) ? section : "workspace";

  return (
    <div className="mx-auto max-w-6xl">
      <div className="mb-7 flex flex-col gap-2 border-b border-border pb-6 sm:flex-row sm:items-end sm:justify-between">
        <div>
          <p className="text-[10px] font-semibold tracking-[.16em] text-success uppercase">Workspace administration</p>
          <h2 className="mt-2 text-2xl font-semibold tracking-tight text-foreground">Settings</h2>
          <p className="mt-1 max-w-2xl text-sm text-secondary">Manage your workspace, access controls, security records, and developer integrations.</p>
        </div>
        <div className="flex items-center gap-2 rounded-md border border-border bg-card px-3 py-2 text-xs text-muted">
          <LockKeyhole className="h-3.5 w-3.5 text-success" />
          {canWrite ? "Workspace administrator" : "Read-only workspace access"}
        </div>
      </div>

      <div className="grid gap-6 lg:grid-cols-[220px_minmax(0,1fr)]">
        <SettingsNavigation activeSection={activeSection} canWrite={canWrite} onSelect={(next) => navigate(`/settings/${next}`)} />
        <section className="min-w-0">
          <SettingsSectionHeader section={activeSection} />
          {activeSection === "workspace" && <Organization />}
          {activeSection === "access" && (canWrite ? <UsersPage /> : <RestrictedAccess />)}
          {activeSection === "security" && <ActivityPage />}
          {activeSection === "developer" && <DeveloperSettings />}
        </section>
      </div>
    </div>
  );
}

type SettingsSection = "workspace" | "access" | "security" | "developer";

const SETTINGS_SECTIONS: {
  key: SettingsSection;
  label: string;
  description: string;
  icon: typeof Building2;
  adminOnly?: boolean;
}[] = [
  { key: "workspace", label: "Workspace", description: "Organization, plan, usage, and domains", icon: Building2 },
  { key: "access", label: "Access", description: "Members, roles, and invitations", icon: Users, adminOnly: true },
  { key: "security", label: "Security", description: "Audit trail and operational history", icon: ShieldCheck },
  { key: "developer", label: "Developer", description: "API tokens, MCP, and connectivity", icon: Code2 },
];

function isSettingsSection(value: string | undefined): value is SettingsSection {
  return SETTINGS_SECTIONS.some((section) => section.key === value);
}

function SettingsNavigation({ activeSection, canWrite, onSelect }: { activeSection: SettingsSection; canWrite: boolean; onSelect: (section: SettingsSection) => void }) {
  return (
    <nav aria-label="Settings sections" className="h-fit rounded-lg border border-border bg-card p-2 lg:sticky lg:top-6">
      <div className="hidden lg:block px-3 pb-2 pt-2 text-[10px] font-semibold uppercase tracking-[.14em] text-muted">Configuration</div>
      <div className="flex gap-1 overflow-x-auto lg:flex-col">
        {SETTINGS_SECTIONS.map((section) => {
          const Icon = section.icon;
          const active = activeSection === section.key;
          return (
            <button
              key={section.key}
              type="button"
              onClick={() => onSelect(section.key)}
              className={`group flex min-w-[148px] items-center gap-3 rounded-md px-3 py-2.5 text-left transition-base lg:min-w-0 ${active ? "bg-[linear-gradient(90deg,rgba(74,222,128,.12),rgba(74,222,128,.03))] text-foreground shadow-[inset_2px_0_0_#4ade80]" : "text-secondary hover:bg-hover hover:text-foreground"}`}
            >
              <Icon className={`h-4 w-4 shrink-0 ${active ? "text-success" : "text-muted group-hover:text-success"}`} />
              <span className="min-w-0 flex-1">
                <span className="block text-sm font-medium">{section.label}</span>
                <span className="mt-0.5 hidden text-[11px] leading-4 text-muted lg:block">{section.description}</span>
              </span>
              {section.adminOnly && !canWrite ? <LockKeyhole className="h-3.5 w-3.5 text-muted" /> : <ChevronRight className={`h-3.5 w-3.5 ${active ? "text-success" : "text-muted opacity-0 group-hover:opacity-100"}`} />}
            </button>
          );
        })}
      </div>
    </nav>
  );
}

function SettingsSectionHeader({ section }: { section: SettingsSection }) {
  const current = SETTINGS_SECTIONS.find((item) => item.key === section)!;
  const Icon = current.icon;
  return (
    <div className="mb-5 flex items-center gap-3">
      <div className="grid h-10 w-10 place-items-center rounded-md border border-[rgba(74,222,128,.2)] bg-[rgba(74,222,128,.07)]"><Icon className="h-5 w-5 text-success" /></div>
      <div><h3 className="text-lg font-semibold text-foreground">{current.label}</h3><p className="text-sm text-muted">{current.description}</p></div>
    </div>
  );
}

function RestrictedAccess() {
  return (
    <Card>
      <CardHeader title="Access controls" subtitle="Members, roles, and invitations are managed by workspace administrators." action={<LockKeyhole className="h-5 w-5 text-muted" />} />
      <div className="rounded-md border border-border bg-surface p-4 text-sm text-secondary">Your viewer role can inspect workspace activity and use personal developer credentials, but it cannot change membership or send invitations.</div>
    </Card>
  );
}

function DeveloperSettings() {
  return (
    <div className="space-y-6">
      <GeneralTab />
      <DevicesAndTokens />
      <McpTab />
    </div>
  );
}
