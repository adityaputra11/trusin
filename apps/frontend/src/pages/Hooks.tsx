import { useEffect, useState, type FormEvent } from "react";
import { useNavigate, useSearchParams } from "react-router-dom";
import { useCanWrite } from "../lib/user-context";
import { Webhook, Plus, Pencil, Trash2 } from "lucide-react";
import { useRules, useCreateRule, useDeleteRule, useUpdateRule, useDestinations, useRuleHealth } from "../lib/hooks";
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
  Textarea,
  Table,
  TBody,
  TD,
  TH,
  THead,
  TR,
  ConfirmDialog,
} from "../components/ui";
import type { ForwardRule, RuleHealth } from "../types/api";

interface FormState {
  name: string;
  provider_id: string;
  target_url: string;
  trigger_on: "success" | "failure";
  method: string;
  headers_text: string;
  signing_secret: string;
  destination_type: "webhook" | "slack" | "telegram" | "email";
}

const METHODS = ["POST", "PUT", "PATCH", "GET", "DELETE"] as const;
const EMPTY: FormState = {
  name: "",
  provider_id: "",
  target_url: "",
  trigger_on: "success",
  method: "POST",
  headers_text: "",
  signing_secret: "",
  destination_type: "webhook",
};

function parseHeadersText(text: string): Record<string, string> {
  return Object.fromEntries(
    text.split("\n").flatMap((line) => {
      const separator = line.indexOf(":");
      if (separator <= 0) return [];
      const key = line.slice(0, separator).trim();
      return key ? [[key, line.slice(separator + 1).trim()]] : [];
    }),
  );
}

function headersToText(headers: Record<string, string> | null | undefined): string {
  return Object.entries(headers ?? {}).map(([key, value]) => `${key}: ${value}`).join("\n");
}

function HealthBadge({ health }: { health?: RuleHealth }) {
  if (!health || health.received_24h === 0) return <Badge variant="neutral">No activity</Badge>;
  if (health.failed_24h > 0) return <Badge variant="danger">Failing</Badge>;
  if (health.delivered_24h > 0) return <Badge variant="success">Healthy</Badge>;
  return <Badge variant="info">In flight</Badge>;
}

export function Hooks() {
  const navigate = useNavigate();
  const [searchParams, setSearchParams] = useSearchParams();
  const canWrite = useCanWrite();
  const { data: rules, isLoading } = useRules();
  const createRule = useCreateRule();
  const deleteRule = useDeleteRule();
  const updateRule = useUpdateRule();
  const { data: destinations = [] } = useDestinations();
  const { data: health = [] } = useRuleHealth();

  const [open, setOpen] = useState(false);
  const [editing, setEditing] = useState<ForwardRule | null>(null);
  const [form, setForm] = useState<FormState>(EMPTY);
  const [error, setError] = useState<string | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<ForwardRule | null>(null);
  const [deleteConfirmation, setDeleteConfirmation] = useState("");

  const providers = (rules ?? []).filter((rule) => rule.rule_kind === "provider");
  const hooks = (rules ?? []).filter((rule) => rule.rule_kind === "hook");
  const nativeDestinations = destinations.filter((destination) => destination.enabled);
  const healthByRule = new Map(health.map((item) => [item.rule_id, item]));
  const selectedProvider = providers.find((provider) => provider.id === form.provider_id);
  const resendEmailLoopRisk = selectedProvider?.source_pattern.toLowerCase() === "resend";

  const openCreate = () => {
    setEditing(null);
    setForm(EMPTY);
    setError(null);
    setOpen(true);
  };

  useEffect(() => {
    if (searchParams.get("new") !== "1" || !canWrite || providers.length === 0) return;
    openCreate();
    setSearchParams({}, { replace: true });
  }, [canWrite, providers.length, searchParams, setSearchParams]);

  const openEdit = (rule: ForwardRule) => {
    setEditing(rule);
    setForm({
      name: rule.name,
      provider_id: rule.provider_id ?? "",
      target_url: rule.target_url,
      trigger_on: rule.trigger_on === "failure" ? "failure" : "success",
      method: METHODS.includes(rule.method as (typeof METHODS)[number]) ? rule.method : "POST",
      headers_text: headersToText(rule.headers),
      signing_secret: "",
      destination_type: rule.destination_type === "slack" || rule.destination_type === "telegram" || rule.destination_type === "email" ? rule.destination_type : "webhook",
    });
    setError(null);
    setOpen(true);
  };

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setError(null);
    try {
      const input = {
        name: form.name.trim(),
        target_url: form.destination_type === "webhook" ? form.target_url.trim() : "",
        provider_id: form.provider_id,
        trigger_on: form.trigger_on,
        method: form.method,
        headers: parseHeadersText(form.headers_text),
        signing_secret: form.signing_secret.trim() || undefined,
        destination_type: form.destination_type,
      };
      if (editing) {
        await updateRule.mutateAsync({ id: editing.id, ...input });
      } else {
        await createRule.mutateAsync({ ...input, rule_kind: "hook" });
      }
      setForm(EMPTY);
      setOpen(false);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to create");
    }
  };

  return (
    <Card className="p-0 overflow-hidden">
      <div className="flex items-center justify-between p-4 border-b border-border">
        <div className="flex items-center gap-2">
          <Webhook className="h-5 w-5 text-muted" />
          <h3 className="text-sm font-semibold text-foreground">
            {hooks.length} hook{hooks.length === 1 ? "" : "s"}
          </h3>
        </div>
        <Button
          size="sm"
          onClick={openCreate}
          hidden={!canWrite}
          disabled={providers.length === 0}
          title={providers.length === 0 ? "Create a provider before adding a hook" : undefined}
        >
          <Plus className="h-4 w-4" /> Add Hook
        </Button>
      </div>

      {isLoading ? (
        <FullSpinner label="Loading hooks…" />
      ) : hooks.length === 0 ? (
        <EmptyState
          icon={<Webhook className="h-10 w-10" strokeWidth={1.5} />}
          title="No hooks configured"
          description="Create a provider first, then add optional success or failure follow-up deliveries."
          action={<Button onClick={() => providers.length ? openCreate() : navigate("/providers?new=1")}><Plus className="h-4 w-4" /> {providers.length ? "Add hook" : "Add provider"}</Button>}
        />
      ) : (
        <Table>
          <THead>
            <TH>Name</TH>
            <TH>Provider</TH>
            <TH>Run when</TH>
            <TH>Target URL</TH>
            <TH>Destination</TH>
            <TH>Status</TH>
            <TH>Health (24h)</TH>
            {canWrite && <TH className="text-right">Actions</TH>}
          </THead>
          <TBody>
            {hooks.map((rule) => (
              <TR key={rule.id}>
                <TD>
                  <span className="font-medium text-foreground">
                    {rule.name}
                  </span>
                  {rule.name === "Default" && (
                    <Badge variant="purple" className="ml-2">
                      default
                    </Badge>
                  )}
                </TD>
                <TD>
                  <code className="text-xs text-secondary font-mono">
                    {providers.find((provider) => provider.id === rule.provider_id)?.name ?? rule.source_pattern}
                  </code>
                </TD>
                <TD>
                  <Badge variant={rule.trigger_on === "failure" ? "warning" : "success"}>
                    {rule.trigger_on === "failure" ? "failure" : "success"}
                  </Badge>
                </TD>
                <TD>
                  <code className="text-xs text-muted font-mono truncate max-w-[260px] inline-block align-bottom">
                    {rule.target_url || "—"}
                  </code>
                </TD>
                <TD><Badge variant="purple">{rule.destination_type ?? "webhook"}</Badge></TD>
                <TD>
                  <button
                    type="button"
                    disabled={!canWrite}
                    onClick={() =>
                      canWrite &&
                      updateRule.mutate({
                        id: rule.id,
                        active: !rule.active,
                      })
                    }
                    className="disabled:cursor-default"
                    title={
                      canWrite
                        ? `Click to ${rule.active ? "disable" : "enable"}`
                        : undefined
                    }
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
        title={editing ? "Edit Hook" : "Add Hook"}
        description="Send the original payload to an additional target after a provider succeeds or fails."
        footer={
          <>
            <Button variant="ghost" onClick={() => setOpen(false)}>
              Cancel
            </Button>
            <Button
              type="submit"
              form="hook-form"
              loading={createRule.isPending || updateRule.isPending}
            >
              {editing ? "Save changes" : <><Plus className="h-4 w-4" /> Create hook</>}
            </Button>
          </>
        }
      >
        <form id="hook-form" onSubmit={submit} className="space-y-4">
          <Field label="Name" htmlFor="hook-name">
            <Input
              id="hook-name"
              value={form.name}
              onChange={(e) => setForm({ ...form, name: e.target.value })}
              placeholder="my-hook"
              required
              autoFocus
            />
          </Field>
          <Field label="Provider" htmlFor="hook-provider" hint="The hook mirrors this provider's source.">
            <Select
              id="hook-provider"
              value={form.provider_id}
              onChange={(e) => setForm({ ...form, provider_id: e.target.value })}
              required
            >
              <option value="" disabled>Select a provider</option>
              {providers.map((provider) => <option key={provider.id} value={provider.id}>{provider.name} ({provider.source_pattern})</option>)}
            </Select>
          </Field>
          <Field label="Destination" htmlFor="hook-destination" hint="Webhook retains the original payload; native destinations send a summary and payload.">
            <Select
              id="hook-destination"
              value={form.destination_type}
              onChange={(event) => setForm({ ...form, destination_type: event.target.value as FormState["destination_type"] })}
            >
              <option value="webhook">Webhook</option>
              {nativeDestinations.map((destination) => <option key={destination.kind} value={destination.kind} disabled={destination.kind === "email" && resendEmailLoopRisk}>{destination.kind === "email" && resendEmailLoopRisk ? "Email (unavailable for Resend)" : destination.kind[0].toUpperCase() + destination.kind.slice(1)}</option>)}
            </Select>
          </Field>
          {resendEmailLoopRisk && form.destination_type === "email" && (
            <p className="rounded-md border border-warning/30 bg-warning/10 p-3 text-sm text-warning">
              Email hooks are unavailable for Resend events because sending an email creates another Resend event.
            </p>
          )}
          {form.destination_type === "webhook" ? <>
            <Field label="Target URL" htmlFor="hook-target">
              <Input id="hook-target" value={form.target_url} onChange={(e) => setForm({ ...form, target_url: e.target.value })} placeholder="https://example.com/webhook" required />
            </Field>
          </> : (
            <div className="rounded-md border border-border bg-surface-2 p-3 text-sm text-secondary">This hook uses the workspace {form.destination_type} destination. Configure or test it in Settings → Destinations.</div>
          )}
          <details className="rounded-md border border-border bg-surface p-3">
            <summary className="cursor-pointer text-sm font-medium text-secondary hover:text-foreground">Advanced settings</summary>
            <div className="mt-4 space-y-4">
              <Field label="Run when" htmlFor="hook-trigger" hint="Failure runs after all provider retries are exhausted.">
                <Select id="hook-trigger" value={form.trigger_on} onChange={(event) => setForm({ ...form, trigger_on: event.target.value as FormState["trigger_on"] })}>
                  <option value="success">Provider succeeds</option>
                  <option value="failure">Provider fails</option>
                </Select>
              </Field>
              {form.destination_type === "webhook" && <>
                <Field label="Outbound method" htmlFor="hook-method">
                  <Select id="hook-method" value={form.method} onChange={(event) => setForm({ ...form, method: event.target.value })}>{METHODS.map((method) => <option key={method} value={method}>{method}</option>)}</Select>
                </Field>
                <Field label="Custom headers" htmlFor="hook-headers" hint='One "Key: value" per line.'>
                  <Textarea id="hook-headers" value={form.headers_text} onChange={(event) => setForm({ ...form, headers_text: event.target.value })} placeholder={"X-Custom-Header: value\nAuthorization: Bearer ..."} rows={3} />
                </Field>
                <Field label="Signing secret" htmlFor="hook-signing" hint={editing ? "Leave blank to retain the existing secret." : "Optional HMAC-SHA256 signing secret."}>
                  <Input id="hook-signing" value={form.signing_secret} onChange={(event) => setForm({ ...form, signing_secret: event.target.value })} placeholder={editing ? "existing secret is hidden" : "leave empty to disable signing"} />
                </Field>
              </>}
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
        title="Delete hook"
        description={`Delete hook "${deleteTarget?.name ?? ""}"? This cannot be undone.`}
        confirmLabel="Delete"
        danger
        loading={deleteRule.isPending}
        confirmDisabled={deleteConfirmation !== (deleteTarget?.target_url ?? "")}
        onConfirm={() => {
          if (deleteTarget) {
            deleteRule.mutate(deleteTarget.id, {
              onSuccess: () => { setDeleteTarget(null); setDeleteConfirmation(""); },
            });
          }
        }}
      >
        <div className="space-y-3">
          <p className="text-sm text-secondary">Type this target URL to confirm:</p>
          <code className="block rounded-md bg-surface-2 px-3 py-2 text-xs text-foreground break-all">{deleteTarget?.target_url}</code>
          <Input value={deleteConfirmation} onChange={(event) => setDeleteConfirmation(event.target.value)} placeholder="Enter target URL" autoComplete="off" />
        </div>
      </ConfirmDialog>
    </Card>
  );
}
