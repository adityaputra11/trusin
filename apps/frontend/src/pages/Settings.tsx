import { useMemo, useState } from "react";
import { useNavigate, useParams, useSearchParams } from "react-router-dom";
import {
  Activity,
  Building2,
  Code2,
  Copy,
  Check,
  Terminal,
  Plug,
  Server,
  Wrench,
  Trash2,
  Plus,
  KeyRound,
  AlertTriangle,
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

// The MCP server is a stdio process — its public tool contract is static.
const MCP_TOOLS = [
  {
    name: "list_events",
    description: "Find webhook events with delivery filters",
    args: "search?, status?, source?, page?, per_page?",
  },
  {
    name: "get_event",
    description: "Inspect one webhook event",
    args: "id: string (required)",
  },
  {
    name: "get_delivery_attempts",
    description: "Inspect an event delivery timeline",
    args: "id: string (required)",
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
    name: "get_metrics",
    description: "Review delivery metrics",
    args: "range?: 24h | 7d | 30d",
  },
  {
    name: "get_health",
    description: "Check relay health and readiness",
    args: "—",
  },
] as const;

type ClientKey = "claude" | "cursor" | "vscode" | "opencode";

const CLIENTS: { key: ClientKey; label: string; file: string }[] = [
  { key: "claude", label: "Claude Desktop", file: "claude_desktop_config.json" },
  { key: "cursor", label: "Cursor", file: ".cursor/mcp.json" },
  { key: "vscode", label: "VS Code (Copilot)", file: "settings.json" },
  { key: "opencode", label: "OpenCode", file: "opencode.json" },
];

function buildSnippet(client: ClientKey): string {
  const server = { command: "trusin", args: ["mcp"] };
  switch (client) {
    case "claude":
      return JSON.stringify({ mcpServers: { trusin: server } }, null, 2);
    case "cursor":
      return JSON.stringify({ mcpServers: { trusin: server } }, null, 2);
    case "vscode":
      // VS Code Copilot Chat uses chat.mcp.discovery.enabled + a servers map
      // under "mcp.servers" (preview). We document the most common shape.
      return JSON.stringify(
        {
          "chat.mcp.discovery.enabled": true,
          "mcp.servers": { trusin: server },
        },
        null,
        2,
      );
    case "opencode":
      return JSON.stringify(
        {
          $schema: "https://opencode.ai/config.json",
          mcp: {
            trusin: {
              type: "local",
              command: ["/usr/local/bin/trusin", "mcp"],
              enabled: true,
              timeout: 10000,
            },
          },
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
              Then run: <code className="text-foreground font-mono">trusin set-token {created.slice(0, 6)}…</code>
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

    </div>
  );
}

function CliInstallCard() {
  const installCommand = "curl -fsSL https://download.trusin.my.id/install.sh | sh";

  return (
    <Card>
      <CardHeader title="Install the CLI" subtitle="Use trusin from your terminal or local development environment" action={<Badge variant="purple">macOS + Linux</Badge>} />
      <div className="space-y-4">
        <CodeBlock code={installCommand} />
        <p className="text-sm text-secondary">
          Supports Apple Silicon and Intel Macs, plus x86_64 and ARM64 Linux. The installer verifies the release checksum before installing to <code className="text-xs text-foreground font-mono">/usr/local/bin</code>.
        </p>
        <div className="grid gap-2 text-xs text-muted sm:grid-cols-2">
          <p>Pin a release: <code className="text-foreground font-mono break-all">curl … | TERUSIN_VERSION=vX.Y.Z sh</code></p>
          <p>Choose a directory: <code className="text-foreground font-mono break-all">curl … | TERUSIN_INSTALL=~/.local/bin sh</code></p>
        </div>
        <p className="text-sm text-secondary">
          Create an API token below, then connect this device with <code className="text-xs text-foreground font-mono">trusin set-token ts_…</code>.
        </p>
      </div>
    </Card>
  );
}

function McpTab() {
  const [client, setClient] = useState<ClientKey>("claude");
  const snippet = useMemo(() => buildSnippet(client), [client]);
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
            <li>Install the CLI above.</li>
            <li>Save your API token with <code className="text-foreground font-mono text-xs">trusin set-token ts_…</code>.</li>
            <li>Copy snippet into your client's config file.</li>
            <li>Restart the AI client.</li>
          </ol>
        </div>

        <div className="flex items-center gap-3 text-sm">
          <Activity className="h-4 w-4 text-muted" />
          <span className="text-secondary">Verify:</span>
          <code className="text-xs font-mono text-foreground bg-surface border border-border rounded px-2 py-1">echo '{`{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}`} ' | trusin mcp</code>
        </div>
      </div>
    </Card>
  );
}

export function Settings() {
  const navigate = useNavigate();
  const { section } = useParams();
  const [searchParams] = useSearchParams();
  const canWrite = useCanWrite();
  const activeSection = isSettingsSection(section) && (section !== "access" || canWrite)
    ? section
    : "workspace";
  const showActivity = searchParams.get("panel") === "activity";

  return (
    <div className="mx-auto max-w-6xl">
      <SettingsNavigation activeSection={activeSection} canWrite={canWrite} onSelect={(next) => navigate(`/settings/${next}`)} />
      <section className="mt-6 min-w-0">
        {activeSection === "workspace" && <><Organization /><WorkspaceActivityPanel defaultOpen={showActivity} /></>}
        {activeSection === "access" && <UsersPage />}
        {activeSection === "developer" && <DeveloperSettings />}
      </section>
    </div>
  );
}

type SettingsSection = "workspace" | "access" | "developer";

const SETTINGS_SECTIONS: {
  key: SettingsSection;
  label: string;
  description: string;
  icon: typeof Building2;
  adminOnly?: boolean;
}[] = [
  { key: "workspace", label: "Workspace", description: "Plan, usage, domains, and activity", icon: Building2 },
  { key: "access", label: "Team", description: "Members, roles, and invitations", icon: Users, adminOnly: true },
  { key: "developer", label: "Developer", description: "API tokens, MCP, and relay health", icon: Code2 },
];

function isSettingsSection(value: string | undefined): value is SettingsSection {
  return SETTINGS_SECTIONS.some((section) => section.key === value);
}

function SettingsNavigation({ activeSection, canWrite, onSelect }: { activeSection: SettingsSection; canWrite: boolean; onSelect: (section: SettingsSection) => void }) {
  const visibleSections = SETTINGS_SECTIONS.filter((section) => !section.adminOnly || canWrite);
  return (
    <nav aria-label="Settings sections" className="rounded-lg border border-border bg-card p-1.5">
      <select
        className="w-full rounded-md border border-border bg-surface px-3 py-2 text-sm text-foreground lg:hidden"
        value={activeSection}
        onChange={(event) => onSelect(event.target.value as SettingsSection)}
      >
        {visibleSections.map((section) => <option key={section.key} value={section.key}>{section.label}</option>)}
      </select>
      <div className="hidden gap-1 lg:flex">
        {visibleSections.map((section) => {
          const Icon = section.icon;
          const active = activeSection === section.key;
          return (
            <button
              key={section.key}
              type="button"
              onClick={() => onSelect(section.key)}
              className={`group flex items-center gap-2 rounded-md px-3 py-2 text-sm font-medium transition-base ${active ? "bg-[rgba(74,222,128,.1)] text-foreground" : "text-secondary hover:bg-hover hover:text-foreground"}`}
            >
              <Icon className={`h-4 w-4 shrink-0 ${active ? "text-success" : "text-muted group-hover:text-success"}`} />
              {section.label}
            </button>
          );
        })}
      </div>
    </nav>
  );
}

function WorkspaceActivityPanel({ defaultOpen }: { defaultOpen: boolean }) {
  const [open, setOpen] = useState(defaultOpen);
  return (
    <Card className="mt-6">
      <CardHeader
        title="Activity"
        subtitle="Audit history for workspace changes and sign-ins."
        action={<Button variant="outline" size="sm" onClick={() => setOpen((value) => !value)}>{open ? "Hide activity" : "View activity"}</Button>}
      />
      {open && <ActivityPage />}
    </Card>
  );
}

function DeveloperSettings() {
  return (
    <div className="space-y-6">
      <GeneralTab />
      <CliInstallCard />
      <DevicesAndTokens />
      <McpTab />
    </div>
  );
}
