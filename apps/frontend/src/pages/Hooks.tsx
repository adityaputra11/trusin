import { useState, type FormEvent } from "react";
import { useCanWrite } from "../lib/user-context";
import { Webhook, Plus, Pencil, Trash2 } from "lucide-react";
import { useRules, useCreateRule, useDeleteRule, useUpdateRule } from "../lib/hooks";
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
import type { ForwardRule } from "../types/api";

interface FormState {
  name: string;
  provider_id: string;
  target_url: string;
  trigger_on: "success" | "failure";
  method: string;
  headers_text: string;
  signing_secret: string;
  destination_type: "webhook" | "slack" | "telegram" | "email";
  slack_webhook_url: string;
  telegram_bot_token: string;
  telegram_chat_id: string;
  email_recipient: string;
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
  slack_webhook_url: "",
  telegram_bot_token: "",
  telegram_chat_id: "",
  email_recipient: "",
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

export function Hooks() {
  const canWrite = useCanWrite();
  const { data: rules, isLoading } = useRules();
  const createRule = useCreateRule();
  const deleteRule = useDeleteRule();
  const updateRule = useUpdateRule();

  const [open, setOpen] = useState(false);
  const [editing, setEditing] = useState<ForwardRule | null>(null);
  const [form, setForm] = useState<FormState>(EMPTY);
  const [error, setError] = useState<string | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<ForwardRule | null>(null);
  const [deleteConfirmation, setDeleteConfirmation] = useState("");

  const providers = (rules ?? []).filter((rule) => rule.rule_kind === "provider");
  const hooks = (rules ?? []).filter((rule) => rule.rule_kind === "hook");
  const destinationNeedsConfig = !editing || editing.destination_type !== form.destination_type;

  const openCreate = () => {
    setEditing(null);
    setForm(EMPTY);
    setError(null);
    setOpen(true);
  };

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
      slack_webhook_url: "",
      telegram_bot_token: "",
      telegram_chat_id: "",
      email_recipient: rule.destination_type === "email" ? rule.target_url : "",
    });
    setError(null);
    setOpen(true);
  };

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setError(null);
    try {
      let raw_destination_config: Record<string, string> | undefined;
      if (form.destination_type === "slack") {
        raw_destination_config = { webhook_url: form.slack_webhook_url.trim() };
      } else if (form.destination_type === "telegram") {
        raw_destination_config = {
          bot_token: form.telegram_bot_token.trim(),
          chat_id: form.telegram_chat_id.trim(),
        };
      } else if (form.destination_type === "email") {
        raw_destination_config = { recipient: form.email_recipient.trim() };
      }
      const destination_config = raw_destination_config && Object.values(raw_destination_config).some(Boolean)
        ? raw_destination_config
        : undefined;
      const input = {
        name: form.name.trim(),
        target_url: form.destination_type === "webhook" ? form.target_url.trim() : "",
        provider_id: form.provider_id,
        trigger_on: form.trigger_on,
        method: form.method,
        headers: parseHeadersText(form.headers_text),
        signing_secret: form.signing_secret.trim() || undefined,
        destination_type: form.destination_type,
        destination_config,
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
          <Field label="Run when" htmlFor="hook-trigger" hint="Failure runs only after all provider retries are exhausted.">
            <Select
              id="hook-trigger"
              value={form.trigger_on}
              onChange={(e) => setForm({ ...form, trigger_on: e.target.value as FormState["trigger_on"] })}
            >
              <option value="success">Provider succeeds</option>
              <option value="failure">Provider fails</option>
            </Select>
          </Field>
          <Field label="Destination" htmlFor="hook-destination" hint="Webhook retains the original payload; native destinations send a summary and payload.">
            <Select
              id="hook-destination"
              value={form.destination_type}
              onChange={(event) => setForm({ ...form, destination_type: event.target.value as FormState["destination_type"] })}
            >
              <option value="webhook">Webhook</option>
              <option value="slack">Slack</option>
              <option value="telegram">Telegram</option>
              <option value="email">Email</option>
            </Select>
          </Field>
          {form.destination_type === "webhook" ? <>
            <Field label="Target URL" htmlFor="hook-target">
              <Input id="hook-target" value={form.target_url} onChange={(e) => setForm({ ...form, target_url: e.target.value })} placeholder="https://example.com/webhook" required />
            </Field>
            <Field label="Outbound method" htmlFor="hook-method">
            <Select
              id="hook-method"
              value={form.method}
              onChange={(e) => setForm({ ...form, method: e.target.value })}
            >
              {METHODS.map((method) => <option key={method} value={method}>{method}</option>)}
            </Select>
          </Field>
          <Field label="Custom headers" htmlFor="hook-headers" hint='One "Key: value" per line.'>
            <Textarea
              id="hook-headers"
              value={form.headers_text}
              onChange={(e) => setForm({ ...form, headers_text: e.target.value })}
              placeholder={"X-Custom-Header: value\nAuthorization: Bearer ..."}
              rows={3}
            />
          </Field>
          <Field label="Signing secret" htmlFor="hook-signing" hint={editing ? "Leave blank to retain the existing secret." : "Optional HMAC-SHA256 signing secret."}>
            <Input
              id="hook-signing"
              value={form.signing_secret}
              onChange={(e) => setForm({ ...form, signing_secret: e.target.value })}
              placeholder={editing ? "existing secret is hidden" : "leave empty to disable signing"}
            />
          </Field>
          </> : form.destination_type === "slack" ? (
            <Field label="Slack Incoming Webhook URL" htmlFor="slack-webhook-url" hint={editing ? "Enter a replacement URL to update this destination." : undefined}>
              <Input id="slack-webhook-url" type="url" value={form.slack_webhook_url} onChange={(event) => setForm({ ...form, slack_webhook_url: event.target.value })} placeholder="https://hooks.slack.com/services/..." required={destinationNeedsConfig} />
            </Field>
          ) : form.destination_type === "telegram" ? <>
            <Field label="Telegram bot token" htmlFor="telegram-bot-token" hint={editing ? "Enter a replacement token to update this destination." : undefined}>
              <Input id="telegram-bot-token" type="password" value={form.telegram_bot_token} onChange={(event) => setForm({ ...form, telegram_bot_token: event.target.value })} required={destinationNeedsConfig} />
            </Field>
            <Field label="Telegram chat ID" htmlFor="telegram-chat-id">
              <Input id="telegram-chat-id" value={form.telegram_chat_id} onChange={(event) => setForm({ ...form, telegram_chat_id: event.target.value })} required={destinationNeedsConfig} />
            </Field>
          </> : (
            <Field label="Email recipient" htmlFor="email-recipient" hint="Uses the server's configured Resend sender.">
              <Input id="email-recipient" type="email" value={form.email_recipient} onChange={(event) => setForm({ ...form, email_recipient: event.target.value })} placeholder="alerts@example.com" required={destinationNeedsConfig} />
            </Field>
          )}
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
