import { useEffect, useState, type FormEvent } from "react";
import { useNavigate, useSearchParams } from "react-router-dom";
import { useCanWrite } from "../lib/user-context";
import { Check, Copy, Plus, Pencil, Settings2, Trash2, FlaskConical } from "lucide-react";
import {
  useRules,
  useCreateRule,
  useDeleteRule,
  useOrganization,
  useUpdateRule,
  useDomains,
  useRuleHealth,
  useDestinations,
} from "../lib/hooks";
import {
  Badge,
  Button,
  Card,
  EmptyState,
  Field,
  FullSpinner,
  Input,
  Modal,
  Select,
  Table,
  TBody,
  TD,
  TH,
  THead,
  TR,
  Textarea,
  ConfirmDialog,
} from "../components/ui";
import type { ForwardRule, RuleHealth } from "../types/api";

const METHODS = ["POST", "PUT", "PATCH", "GET", "DELETE"] as const;

interface FormState {
  name: string;
  target_url: string;
  source_pattern: string;
  method: string;
  headers_text: string; // newline-separated "Key: value" lines, edited as text
  signing_secret: string;
  ingest_hostname: string;
  failure_destination: "" | "slack" | "telegram";
}

const EMPTY: FormState = {
  name: "",
  target_url: "",
  source_pattern: "",
  method: "POST",
  headers_text: "",
  signing_secret: "",
  ingest_hostname: "",
  failure_destination: "",
};

/** Parse "Key: value" lines (one per line) into a headers object. */
function parseHeadersText(text: string): Record<string, string> {
  const out: Record<string, string> = {};
  for (const line of text.split("\n")) {
    const raw = line.trim();
    if (!raw) continue;
    const idx = raw.indexOf(":");
    if (idx <= 0) continue;
    const key = raw.slice(0, idx).trim();
    const val = raw.slice(idx + 1).trim();
    if (key) out[key] = val;
  }
  return out;
}

function headersToText(h: Record<string, string> | null | undefined): string {
  if (!h) return "";
  return Object.entries(h)
    .map(([k, v]) => `${k}: ${v}`)
    .join("\n");
}

function webhookUrl(endpoint: string | undefined, source: string, ingestHostname?: string | null): string {
  if (!source.trim()) return "";
  const base = ingestHostname ? `https://${ingestHostname}` : endpoint;
  if (!base) return "";
  return `${base.replace(/\/$/, "")}/${encodeURIComponent(source.trim())}`;
}

function HealthBadge({ health }: { health?: RuleHealth }) {
  if (!health || health.received_24h === 0) return <Badge variant="neutral">No activity</Badge>;
  if (health.failed_24h > 0) return <Badge variant="danger">Failing</Badge>;
  if (health.delivered_24h > 0) return <Badge variant="success">Healthy</Badge>;
  return <Badge variant="info">In flight</Badge>;
}

export function Providers() {
  const navigate = useNavigate();
  const [searchParams, setSearchParams] = useSearchParams();
  const canWrite = useCanWrite();
  const { data: rules, isLoading } = useRules();
  const { data: organization } = useOrganization();
  const { data: domains } = useDomains(canWrite);
  const { data: health = [] } = useRuleHealth();
  const { data: destinations = [] } = useDestinations();
  const createRule = useCreateRule();
  const updateRule = useUpdateRule();
  const deleteRule = useDeleteRule();

  const [open, setOpen] = useState(false);
  const [editing, setEditing] = useState<ForwardRule | null>(null);
  const [form, setForm] = useState<FormState>(EMPTY);
  const [error, setError] = useState<string | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<ForwardRule | null>(null);
  const [copiedProviderId, setCopiedProviderId] = useState<string | null>(null);
  const [deleteConfirmation, setDeleteConfirmation] = useState("");

  // Providers = named source mappings, exclude the seeded catch-all "Default".
  const providers = (rules ?? []).filter((r) => r.rule_kind === "provider");
  const nativeDestinations = destinations.filter((destination) => destination.enabled);
  const healthByRule = new Map(health.map((item) => [item.rule_id, item]));

  const openCreate = () => {
    setEditing(null);
    setForm(EMPTY);
    setError(null);
    setOpen(true);
  };

  useEffect(() => {
    if (searchParams.get("new") !== "1" || !canWrite) return;
    openCreate();
    setSearchParams({}, { replace: true });
  }, [canWrite, searchParams, setSearchParams]);

  const openEdit = (rule: ForwardRule) => {
    setEditing(rule);
    setForm({
      name: rule.name,
      target_url: rule.target_url,
      source_pattern:
        rule.source_pattern === "*" ? rule.name : rule.source_pattern,
      method: METHODS.includes(rule.method as (typeof METHODS)[number])
        ? rule.method
        : "POST",
      headers_text: headersToText(rule.headers),
      signing_secret: rule.signing_secret ?? "",
      ingest_hostname: rule.ingest_hostname ?? "",
      failure_destination: "",
    });
    setError(null);
    setOpen(true);
  };

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setError(null);
    const source_pattern = form.source_pattern.trim() || form.name.trim();
    const headers = parseHeadersText(form.headers_text);
    try {
      if (editing) {
        await updateRule.mutateAsync({
          id: editing.id,
          target_url: form.target_url.trim(),
          source_pattern,
          method: form.method,
          headers,
          signing_secret: form.signing_secret.trim() || undefined,
          ingest_hostname: form.ingest_hostname,
        });
      } else {
        const provider = await createRule.mutateAsync({
          name: form.name.trim(),
          target_url: form.target_url.trim(),
          source_pattern,
          method: form.method,
          headers,
          signing_secret: form.signing_secret.trim() || undefined,
          rule_kind: "provider",
          ingest_hostname: form.ingest_hostname,
        });
        if (form.failure_destination) {
          await createRule.mutateAsync({
            name: `${form.name.trim()} failure alert`,
            target_url: "",
            rule_kind: "hook",
            provider_id: provider.id,
            trigger_on: "failure",
            destination_type: form.failure_destination,
          });
        }
      }
      setOpen(false);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to save");
    }
  };

  const saving = createRule.isPending || updateRule.isPending;

  const copyWebhookUrl = async (provider: ForwardRule) => {
    const url = webhookUrl(organization?.ingest_url, provider.source_pattern, provider.ingest_hostname);
    if (!url) return;
    try {
      await navigator.clipboard.writeText(url);
      setCopiedProviderId(provider.id);
      window.setTimeout(() => setCopiedProviderId(null), 1500);
    } catch {
      setCopiedProviderId(null);
    }
  };

  return (
    <Card className="p-0 overflow-hidden">
      <div className="flex items-center justify-between p-4 border-b border-border">
        <div className="flex items-center gap-2">
          <Settings2 className="h-5 w-5 text-muted" />
          <h3 className="text-sm font-semibold text-foreground">
            {providers.length} provider{providers.length === 1 ? "" : "s"}
          </h3>
        </div>
        <Button size="sm" onClick={openCreate} hidden={!canWrite}>
          <Plus className="h-4 w-4" /> Add Provider
        </Button>
      </div>

      {isLoading ? (
        <FullSpinner label="Loading providers…" />
      ) : providers.length === 0 ? (
        <EmptyState
          icon={<Settings2 className="h-10 w-10" strokeWidth={1.5} />}
          title="No providers configured"
          description="Providers map a named webhook source (e.g. 'midtrans') to a forwarding target URL."
          action={
            <Button onClick={openCreate}>
              <Plus className="h-4 w-4" /> Add your first provider
            </Button>
          }
        />
      ) : (
        <Table>
          <THead>
            <TH>Name</TH>
            <TH>Source</TH>
            <TH>Webhook URL</TH>
            <TH>Outbound method</TH>
            <TH>Target URL</TH>
            <TH>Status</TH>
            <TH>Health (24h)</TH>
            {canWrite && <TH className="text-right">Actions</TH>}
          </THead>
          <TBody>
            {providers.map((rule) => (
              <TR key={rule.id}>
                <TD>
                  <span className="font-medium text-foreground">
                    {rule.name}
                  </span>
                </TD>
                <TD>
                  <code className="text-xs text-secondary font-mono">
                    {rule.source_pattern}
                  </code>
                </TD>
                <TD>
                  <div className="flex items-center gap-1 max-w-[280px]">
                    <code className="text-xs text-muted font-mono truncate">
                      {webhookUrl(organization?.ingest_url, rule.source_pattern, rule.ingest_hostname) || "Loading…"}
                    </code>
                    <Badge variant="purple" className="shrink-0">POST only</Badge>
                    <button
                      type="button"
                      onClick={() => copyWebhookUrl(rule)}
                      disabled={!organization?.ingest_url}
                      className="p-1.5 rounded-md text-muted hover:text-foreground hover:bg-hover disabled:cursor-not-allowed shrink-0"
                      title="Copy webhook URL"
                      aria-label={`Copy ${rule.name} webhook URL`}
                    >
                      {copiedProviderId === rule.id ? <Check className="h-3.5 w-3.5 text-success" /> : <Copy className="h-3.5 w-3.5" />}
                    </button>
                  </div>
                </TD>
                <TD>
                  <code className="text-xs text-secondary font-mono">
                    {rule.method}
                  </code>
                </TD>
                <TD>
                  <code className="text-xs text-muted font-mono truncate max-w-[280px] inline-block align-bottom">
                    {rule.target_url || "—"}
                  </code>
                </TD>
                <TD>
                  <button
                    type="button"
                    disabled={!canWrite || updateRule.isPending}
                    onClick={() => canWrite && updateRule.mutate({ id: rule.id, active: !rule.active })}
                    className="disabled:cursor-default"
                    title={canWrite ? `${rule.active ? "Pause" : "Enable"} provider delivery` : undefined}
                  >
                    <Badge variant={rule.active ? "success" : "neutral"}>
                      {rule.active ? "enabled" : "paused"}
                    </Badge>
                  </button>
                </TD>
                <TD>
                  <div className="space-y-1">
                    <HealthBadge health={healthByRule.get(rule.id)} />
                    {healthByRule.get(rule.id)?.received_24h ? <p className="text-[11px] text-muted">{healthByRule.get(rule.id)!.delivered_24h}/{healthByRule.get(rule.id)!.received_24h} delivered</p> : null}
                  </div>
                </TD>
                {canWrite && (
                <TD className="text-right">
                  <div className="flex items-center justify-end gap-1">
                    <button
                      onClick={() => navigate(`/send?provider=${encodeURIComponent(rule.id)}`)}
                      className="p-2 rounded-md text-muted hover:text-success hover:bg-hover transition-base"
                      title="Send test webhook"
                    >
                      <FlaskConical className="h-4 w-4" />
                    </button>
                    <button
                      onClick={() => openEdit(rule)}
                      className="p-2 rounded-md text-muted hover:text-foreground hover:bg-hover transition-base"
                      title="Edit"
                    >
                      <Pencil className="h-4 w-4" />
                    </button>
                    <button
                      onClick={() => { setDeleteTarget(rule); setDeleteConfirmation(""); }}
                      className="p-2 rounded-md text-muted hover:text-danger hover:bg-[rgba(239,68,68,.1)] transition-base"
                      title="Delete"
                    >
                      <Trash2 className="h-4 w-4" />
                    </button>
                  </div>
                </TD>
                )}
              </TR>
            ))}
          </TBody>
        </Table>
      )}

      <Modal
        open={open}
        onClose={() => setOpen(false)}
        title={editing ? "Edit Provider" : "Add Provider"}
        description="A provider maps a named source to a forwarding target."
        footer={
          <>
            <Button variant="ghost" onClick={() => setOpen(false)}>
              Cancel
            </Button>
            <Button
              onClick={submit as unknown as () => void}
              loading={saving}
              type="submit"
              form="provider-form"
            >
              {editing ? "Save changes" : "Add provider"}
            </Button>
          </>
        }
      >
        <form id="provider-form" onSubmit={submit} className="space-y-4">
          <Field label="Name" htmlFor="name">
            <Input
              id="name"
              value={form.name}
              onChange={(e) => setForm({ ...form, name: e.target.value })}
              placeholder="midtrans"
              required
              autoFocus
              disabled={!!editing}
            />
          </Field>
          <Field label="Target URL" htmlFor="target">
            <Input
              id="target"
              value={form.target_url}
              onChange={(e) =>
                setForm({ ...form, target_url: e.target.value })
              }
              placeholder="https://example.com/webhook"
              required
            />
          </Field>
          <details className="rounded-md border border-border bg-surface p-3">
            <summary className="cursor-pointer text-sm font-medium text-secondary hover:text-foreground">Advanced settings</summary>
            <div className="mt-4 space-y-4">
              <Field label="Source pattern" htmlFor="source" hint="Defaults to the provider name. Use * to match everything.">
                <Input id="source" value={form.source_pattern} onChange={(e) => setForm({ ...form, source_pattern: e.target.value })} placeholder={form.name || "source-name"} />
              </Field>
              <Field label="Incoming domain" htmlFor="ingest-hostname" hint="Defaults to the canonical Terusin domain.">
                <Select id="ingest-hostname" value={form.ingest_hostname} onChange={(event) => setForm({ ...form, ingest_hostname: event.target.value })}>
                  <option value="">Terusin canonical domain</option>
                  {(domains ?? []).filter((domain) => domain.status === "active").map((domain) => <option key={domain.id} value={domain.hostname}>{domain.hostname}</option>)}
                </Select>
              </Field>
              {!editing && nativeDestinations.length > 0 && (
                <Field label="Failure alerts" htmlFor="provider-failure-destination" hint="Automatically creates a failure notification Hook.">
                  <Select id="provider-failure-destination" value={form.failure_destination} onChange={(event) => setForm({ ...form, failure_destination: event.target.value as FormState["failure_destination"] })}>
                    <option value="">No automatic alert</option>
                    {nativeDestinations.map((destination) => <option key={destination.kind} value={destination.kind}>Alert via {destination.kind[0].toUpperCase() + destination.kind.slice(1)}</option>)}
                  </Select>
                </Field>
              )}
              <Field label="Outbound method" htmlFor="method">
                <Select id="method" value={form.method} onChange={(e) => setForm({ ...form, method: e.target.value })}>{METHODS.map((method) => <option key={method} value={method}>{method}</option>)}</Select>
              </Field>
              <Field label="Custom headers" htmlFor="headers" hint='One "Key: value" per line.'>
                <Textarea id="headers" value={form.headers_text} onChange={(e) => setForm({ ...form, headers_text: e.target.value })} placeholder={"X-Custom-Header: value\nAuthorization: Bearer ..."} rows={3} />
              </Field>
              <Field label="Signing secret" htmlFor="signing" hint="Adds an X-Terusin-Signature header (HMAC-SHA256).">
                <Input id="signing" value={form.signing_secret} onChange={(e) => setForm({ ...form, signing_secret: e.target.value })} placeholder="leave empty to disable signing" />
              </Field>
            </div>
          </details>
          {error && (
            <p className="text-sm text-danger bg-[rgba(239,68,68,.1)] border border-[rgba(239,68,68,.25)] rounded-md p-3">
              {error}
            </p>
          )}
        </form>
      </Modal>

      <ConfirmDialog
        open={deleteTarget !== null}
        onClose={() => { setDeleteTarget(null); setDeleteConfirmation(""); }}
        title="Delete webhook"
        description="This cannot be undone. Attached hooks will also be deleted."
        confirmLabel="Delete webhook"
        danger
        loading={deleteRule.isPending}
        confirmDisabled={deleteConfirmation !== webhookUrl(organization?.ingest_url, deleteTarget?.source_pattern ?? "", deleteTarget?.ingest_hostname)}
        onConfirm={() => {
          if (deleteTarget) {
            deleteRule.mutate(deleteTarget.id, {
              onSuccess: () => { setDeleteTarget(null); setDeleteConfirmation(""); },
            });
          }
        }}
      >
        <div className="space-y-3">
          <p className="text-sm text-secondary">Type this webhook endpoint to confirm:</p>
          <code className="block rounded-md bg-surface-2 px-3 py-2 text-xs text-foreground break-all">
            {webhookUrl(organization?.ingest_url, deleteTarget?.source_pattern ?? "", deleteTarget?.ingest_hostname)}
          </code>
          <Input
            value={deleteConfirmation}
            onChange={(event) => setDeleteConfirmation(event.target.value)}
            placeholder="Enter webhook endpoint"
            autoComplete="off"
          />
        </div>
      </ConfirmDialog>
    </Card>
  );
}
