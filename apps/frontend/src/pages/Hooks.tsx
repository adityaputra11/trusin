import { useState, type FormEvent } from "react";
import { useCanWrite } from "../lib/user-context";
import { Webhook, Plus, Trash2 } from "lucide-react";
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
  Table,
  TBody,
  TD,
  TH,
  THead,
  TR,
} from "../components/ui";

interface FormState {
  name: string;
  source_pattern: string;
  target_url: string;
}

const EMPTY: FormState = { name: "", source_pattern: "", target_url: "" };

export function Hooks() {
  const canWrite = useCanWrite();
  const { data: rules, isLoading } = useRules();
  const createRule = useCreateRule();
  const deleteRule = useDeleteRule();
  const updateRule = useUpdateRule();

  const [open, setOpen] = useState(false);
  const [form, setForm] = useState<FormState>(EMPTY);
  const [error, setError] = useState<string | null>(null);

  const hooks = rules ?? [];

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setError(null);
    try {
      await createRule.mutateAsync({
        name: form.name.trim(),
        source_pattern: form.source_pattern.trim() || "*",
        target_url: form.target_url.trim(),
      });
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
        <Button size="sm" onClick={() => setOpen(true)} hidden={!canWrite}>
          <Plus className="h-4 w-4" /> Add Hook
        </Button>
      </div>

      {isLoading ? (
        <FullSpinner />
      ) : hooks.length === 0 ? (
        <EmptyState
          icon={<Webhook className="h-10 w-10" strokeWidth={1.5} />}
          title="No hooks configured"
          description="Hooks are forwarding rules that route incoming webhooks to target URLs."
        />
      ) : (
        <Table>
          <THead>
            <TH>Name</TH>
            <TH>Source</TH>
            <TH>Target URL</TH>
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
                    {rule.source_pattern}
                  </code>
                </TD>
                <TD>
                  <code className="text-xs text-muted font-mono truncate max-w-[260px] inline-block align-bottom">
                    {rule.target_url || "—"}
                  </code>
                </TD>
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
                      {rule.active ? "active" : "inactive"}
                    </Badge>
                  </button>
                </TD>
                {canWrite && (
                <TD className="text-right">
                  <button
                    onClick={() => {
                      if (confirm(`Delete hook "${rule.name}"?`)) {
                        deleteRule.mutate(rule.id);
                      }
                    }}
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
        title="Add Hook"
        description="Create a new forwarding rule."
        footer={
          <>
            <Button variant="ghost" onClick={() => setOpen(false)}>
              Cancel
            </Button>
            <Button
              type="submit"
              form="hook-form"
              loading={createRule.isPending}
            >
              <Plus className="h-4 w-4" /> Create hook
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
          <Field
            label="Source pattern"
            htmlFor="hook-source"
            hint="Match a source path or * for everything."
          >
            <Input
              id="hook-source"
              value={form.source_pattern}
              onChange={(e) =>
                setForm({ ...form, source_pattern: e.target.value })
              }
              placeholder="*"
            />
          </Field>
          <Field label="Target URL" htmlFor="hook-target">
            <Input
              id="hook-target"
              value={form.target_url}
              onChange={(e) =>
                setForm({ ...form, target_url: e.target.value })
              }
              placeholder="https://example.com/webhook"
              required
            />
          </Field>
          {error && (
            <p className="text-sm text-danger bg-[rgba(239,68,68,.1)] border border-[rgba(239,68,68,.25)] rounded-md p-3">
              {error}
            </p>
          )}
        </form>
      </Modal>
    </Card>
  );
}
