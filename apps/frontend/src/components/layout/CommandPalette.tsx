import { useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import { Activity, Bell, Command, Plus, Send, Settings2, Webhook } from "lucide-react";

const COMMANDS = [
  { label: "Open dashboard", hint: "Recent events", to: "/", icon: Activity },
  { label: "Add provider", hint: "Configure a source", to: "/providers?new=1", icon: Plus },
  { label: "Add hook", hint: "Configure a notification", to: "/hooks?new=1", icon: Webhook },
  { label: "Send test webhook", hint: "Verify a provider", to: "/send", icon: Send },
  { label: "Open destinations", hint: "Slack and Telegram", to: "/settings/destinations", icon: Bell },
  { label: "Open settings", hint: "Workspace controls", to: "/settings/workspace", icon: Settings2 },
] as const;

export function CommandPalette({ open, onClose }: { open: boolean; onClose: () => void }) {
  const navigate = useNavigate();
  const [query, setQuery] = useState("");
  const commands = useMemo(() => {
    const term = query.trim().toLowerCase();
    return term ? COMMANDS.filter((command) => `${command.label} ${command.hint}`.toLowerCase().includes(term)) : COMMANDS;
  }, [query]);

  useEffect(() => {
    if (!open) setQuery("");
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [open, onClose]);

  if (!open) return null;
  const run = (to: string) => {
    navigate(to);
    onClose();
  };
  return (
    <div className="fixed inset-0 z-50 flex items-start justify-center bg-black/65 px-4 pt-[14vh] backdrop-blur-sm" role="dialog" aria-modal="true" aria-label="Command palette" onMouseDown={onClose}>
      <div className="w-full max-w-xl overflow-hidden rounded-xl border border-border-light bg-card shadow-2xl" onMouseDown={(event) => event.stopPropagation()}>
        <div className="flex items-center gap-3 border-b border-border px-4 py-3">
          <Command className="h-5 w-5 text-success" />
          <input autoFocus value={query} onChange={(event) => setQuery(event.target.value)} placeholder="Search commands…" className="min-w-0 flex-1 bg-transparent text-sm text-foreground outline-none placeholder:text-muted" />
          <button type="button" onClick={onClose} className="rounded-md px-1.5 py-1 text-xs text-muted hover:bg-hover hover:text-foreground">Esc</button>
        </div>
        <div className="max-h-[55vh] overflow-y-auto p-2">
          {commands.length ? commands.map((command) => {
            const Icon = command.icon;
            return <button key={command.label} type="button" onClick={() => run(command.to)} className="flex w-full items-center gap-3 rounded-lg px-3 py-3 text-left hover:bg-hover">
              <span className="grid h-8 w-8 place-items-center rounded-md border border-border bg-surface text-success"><Icon className="h-4 w-4" /></span>
              <span className="min-w-0 flex-1"><span className="block text-sm font-medium text-foreground">{command.label}</span><span className="block text-xs text-muted">{command.hint}</span></span>
            </button>;
          }) : <p className="px-3 py-8 text-center text-sm text-muted">No matching commands.</p>}
        </div>
        <div className="flex items-center justify-between border-t border-border px-4 py-2 text-[11px] text-muted"><span><kbd className="rounded border border-border bg-surface px-1.5 py-0.5">↵</kbd> select</span><span><kbd className="rounded border border-border bg-surface px-1.5 py-0.5">Esc</kbd> close</span></div>
      </div>
    </div>
  );
}
