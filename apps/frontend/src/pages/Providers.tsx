import { useState, type FormEvent } from "react";
import { useCanWrite } from "../lib/user-context";
import { Settings2, Plus, Pencil, Trash2 } from "lucide-react";
import { useRules, useCreateRule, useDeleteRule } from "../lib/hooks";
import {
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
import type { ForwardRule } from "../types/api";

interface FormState {
  name: string;
  target_url: string;
  source_pattern: string;
}

const EMPTY: FormState = { name: "", target_url: "", source_pattern: "" };

export function Providers() {
  const canWrite = useCanWrite();
  const { data: rules, isLoading } = useRules();
  const createRule = useCreateRule();
  const deleteRule = useDeleteRule();

  const [open, setOpen] = useState(false);
  const [editing, setEditing] = useState<ForwardRule | null>(null);
  const [form, setForm] = useState<FormState>(EMPTY);
  const [error, setError] = useState<string | null>(null);

  // Providers = named source mappings, exclude the seeded catch-all "Default".
  const providers = (rules ?? []).filter((r) => r.name !== "Default");

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
      target_url: rule.target_url,
      source_pattern:
        rule.source_pattern === "*" ? rule.name : rule.source_pattern,
    });
    setError(null);
    setOpen(true);
  };

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setError(null);
    const source_pattern = form.source_pattern.trim() || form.name.trim();
    // Edit = create-new + delete-old (mirrors current SSR backend behavior).
    try {
      await createRule.mutateAsync({
        name: form.name.trim(),
        target_url: form.target_url.trim(),
        source_pattern,
      });
      if (editing) await deleteRule.mutateAsync(editing.id);
      setOpen(false);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to save");
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
        <FullSpinner />
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
            <TH>Target URL</TH>
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
                  <code className="text-xs text-muted font-mono truncate max-w-[280px] inline-block align-bottom">
                    {rule.target_url || "—"}
                  </code>
                </TD>
                {canWrite && (
                <TD className="text-right">
                  <div className="flex items-center justify-end gap-1">
                    <button
                      onClick={() => openEdit(rule)}
                      className="p-2 rounded-md text-muted hover:text-foreground hover:bg-hover transition-base"
                      title="Edit"
                    >
                      <Pencil className="h-4 w-4" />
                    </button>
                    <button
                      onClick={() => {
                        if (
                          confirm(`Delete provider "${rule.name}"?`)
                        ) {
                          deleteRule.mutate(rule.id);
                        }
                      }}
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
              loading={createRule.isPending}
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
          <Field
            label="Source pattern"
            htmlFor="source"
            hint="Defaults to the provider name. Use * to match everything."
          >
            <Input
              id="source"
              value={form.source_pattern}
              onChange={(e) =>
                setForm({ ...form, source_pattern: e.target.value })
              }
              placeholder={form.name || "source-name"}
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
