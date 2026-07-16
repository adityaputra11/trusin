import { useState } from "react";
import { useParams, useNavigate } from "react-router-dom";
import {
  ArrowLeft,
  RotateCw,
  CheckCircle2,
  XCircle,
  Clock,
  Inbox,
  Trash2,
  History,
  Sparkles,
} from "lucide-react";
import {
  useAiStatus,
  useAttempts,
  useDeleteEvent,
  useEvent,
  useExplainEvent,
  useHookNotifications,
  useRetryEvent,
} from "../lib/hooks";
import { useCanWrite } from "../lib/user-context";
import { StatusBadge } from "../components/StatusBadge";
import {
  Button,
  Card,
  CardHeader,
  FullSpinner,
  EmptyState,
  ConfirmDialog,
} from "../components/ui";
import { formatDateTime, prettyJson } from "../lib/format";
import type { AiExplanation, EventStatus } from "../types/api";

function HeadersList({ headers }: { headers: Record<string, string> | null }) {
  if (!headers || Object.keys(headers).length === 0) {
    return <p className="text-sm text-muted">No headers</p>;
  }
  return (
    <div className="space-y-1">
      {Object.entries(headers).map(([k, v]) => (
        <div key={k} className="flex gap-3 text-xs">
          <span className="text-muted shrink-0 font-mono">{k}:</span>
          <span className="text-secondary font-mono break-all">{v}</span>
        </div>
      ))}
    </div>
  );
}

function PayloadSummary({ body }: { body: unknown }) {
  if (!body || typeof body !== "object" || Array.isArray(body)) return null;
  const root = body as Record<string, unknown>;
  const data = root.data && typeof root.data === "object" && !Array.isArray(root.data)
    ? root.data as Record<string, unknown>
    : {};
  const value = (...keys: string[]) => {
    for (const key of keys) {
      const candidate = root[key] ?? data[key];
      if (typeof candidate === "string" || typeof candidate === "number" || typeof candidate === "boolean") return String(candidate);
    }
    return null;
  };
  const customer = root.customer ?? data.customer;
  const customerLabel = customer && typeof customer === "object" && !Array.isArray(customer)
    ? valueFromCustomer(customer as Record<string, unknown>)
    : value("customer_email", "email");
  const fields = [
    ["Event", value("event", "type", "event_type")],
    ["Status", value("status", "state")],
    ["Amount", value("amount", "total", "value")],
    ["Currency", value("currency")],
    ["Customer", customerLabel],
    ["ID", value("id", "event_id", "reference_id")],
  ].filter((field): field is [string, string] => Boolean(field[1]));
  if (!fields.length) return null;
  return (
    <div className="mb-4 rounded-md border border-border bg-surface p-3">
      <p className="mb-2 text-xs font-medium uppercase text-secondary">Payload summary</p>
      <div className="grid grid-cols-2 gap-x-4 gap-y-2 text-xs sm:grid-cols-3">
        {fields.map(([label, fieldValue]) => <div key={label}><p className="text-muted">{label}</p><p className="mt-0.5 truncate font-medium text-foreground" title={fieldValue}>{fieldValue}</p></div>)}
      </div>
    </div>
  );
}

function valueFromCustomer(customer: Record<string, unknown>) {
  for (const key of ["name", "email", "id"]) {
    const value = customer[key];
    if (typeof value === "string" || typeof value === "number") return String(value);
  }
  return null;
}

function StatusIcon({ status }: { status: EventStatus }) {
  if (status === "delivered")
    return <CheckCircle2 className="h-4 w-4 text-success" />;
  if (status === "failed") return <XCircle className="h-4 w-4 text-danger" />;
  if (status === "retrying")
    return <RotateCw className="h-4 w-4 text-warning" />;
  return <Clock className="h-4 w-4 text-info" />;
}

export function EventDetail() {
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const { data: ev, isLoading } = useEvent(id);
  const retry = useRetryEvent();
  const deleteEvent = useDeleteEvent();
  const canWrite = useCanWrite();
  const { data: attempts } = useAttempts(id, ev?.status);
  const { data: notifications } = useHookNotifications(id, ev?.status);
  const { data: aiStatus } = useAiStatus();
  const explain = useExplainEvent();
  const [confirmDelete, setConfirmDelete] = useState(false);

  if (isLoading) return <FullSpinner label="Loading event…" />;

  if (!ev) {
    return (
      <EmptyState
        icon={<Inbox className="h-10 w-10" strokeWidth={1.5} />}
        title="Event not found"
        description="This event may have been deleted, or the ID is invalid."
        action={
          <Button variant="outline" onClick={() => navigate("/")}>
            <ArrowLeft className="h-4 w-4" /> Back to dashboard
          </Button>
        }
      />
    );
  }

  return (
    <div>
      <div className="flex items-center justify-between mb-6">
        <Button variant="ghost" size="sm" onClick={() => navigate("/")}>
          <ArrowLeft className="h-4 w-4" /> Back
        </Button>
        <div className="flex items-center gap-2">
        {aiStatus?.enabled && (
          <Button
            variant="outline"
            size="sm"
            onClick={() => explain.mutate(ev.id)}
            loading={explain.isPending}
          >
            <Sparkles className="h-4 w-4" /> Explain with AI
          </Button>
        )}
        {canWrite && (
          <>
            <Button
              variant="success"
              size="sm"
              onClick={() => retry.mutate(ev.id)}
              loading={retry.isPending}
            >
              <RotateCw className="h-4 w-4" /> Retry
            </Button>
            <Button
              variant="danger"
              size="sm"
              loading={deleteEvent.isPending}
              onClick={() => setConfirmDelete(true)}
            >
              <Trash2 className="h-4 w-4" /> Delete
            </Button>
          </>
        )}
        </div>
      </div>

      <Card className="mb-6">
        <CardHeader title={`Event ${ev.id}`} subtitle={formatDateTime(ev.created_at)} />
        <div className="grid grid-cols-2 md:grid-cols-4 gap-4 text-sm">
          <div>
            <p className="text-xs text-muted uppercase mb-1">Source</p>
            <p className="text-foreground font-medium">{ev.source}</p>
          </div>
          <div>
            <p className="text-xs text-muted uppercase mb-1">Status</p>
            <div className="flex items-center gap-2">
              <StatusIcon status={ev.status as EventStatus} />
              <StatusBadge status={ev.status as EventStatus} />
            </div>
          </div>
          <div>
            <p className="text-xs text-muted uppercase mb-1">Retry</p>
            <p className="text-foreground font-medium">
              {ev.retry_count} / {ev.max_retries}
            </p>
          </div>
          <div>
            <p className="text-xs text-muted uppercase mb-1">Target</p>
            <code className="text-xs text-secondary font-mono break-all">
              {ev.target_url || "—"}
            </code>
          </div>
        </div>
      </Card>

      {explain.data && <AiExplanationCard explanation={explain.data} />}
      {explain.isError && (
        <Card className="mb-6 border-danger">
          <CardHeader title="AI explanation unavailable" subtitle="Please try again shortly." />
        </Card>
      )}

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        <Card>
          <CardHeader
            title="Request"
            subtitle="Incoming webhook payload"
          />
          <PayloadSummary body={ev.body} />
          <div className="mb-4">
            <p className="text-xs font-medium text-secondary uppercase mb-2">
              Headers
            </p>
            <HeadersList headers={ev.headers} />
          </div>
          <div>
            <p className="text-xs font-medium text-secondary uppercase mb-2">
              Body
            </p>
            <pre className="bg-surface border border-border rounded-md p-3 text-xs text-secondary font-mono overflow-x-auto max-h-80">
              {prettyJson(ev.body)}
            </pre>
          </div>
        </Card>

        <Card>
          <CardHeader
            title="Response"
            subtitle={
              ev.response_status
                ? `HTTP ${ev.response_status}`
                : "No response yet"
            }
          />
          {ev.response_status ? (
            <>
              {ev.response_headers && (
                <div className="mb-4">
                  <p className="text-xs font-medium text-secondary uppercase mb-2">
                    Headers
                  </p>
                  <HeadersList headers={ev.response_headers} />
                </div>
              )}
              <div>
                <p className="text-xs font-medium text-secondary uppercase mb-2">
                  Body
                </p>
                <pre className="bg-surface border border-border rounded-md p-3 text-xs text-secondary font-mono overflow-x-auto max-h-80">
                  {prettyJson(ev.response_body)}
                </pre>
              </div>
            </>
          ) : (
            <EmptyState
              icon={<Clock className="h-8 w-8" strokeWidth={1.5} />}
              title="Pending delivery"
              description="The worker has not produced a response for this event yet."
            />
          )}
        </Card>
      </div>

      <DeliveryTimeline attempts={attempts ?? []} />
      <HookNotificationsTimeline notifications={notifications ?? []} />

      <ConfirmDialog
        open={confirmDelete}
        onClose={() => setConfirmDelete(false)}
        title="Delete event"
        description="Delete this event permanently? This cannot be undone."
        confirmLabel="Delete"
        danger
        loading={deleteEvent.isPending}
        onConfirm={() =>
          deleteEvent.mutate(ev.id, {
            onSuccess: () => navigate("/"),
          })
        }
      />
    </div>
  );
}

function HookNotificationsTimeline({ notifications }: { notifications: import("../types/api").HookNotificationDelivery[] }) {
  if (notifications.length === 0) return null;
  return (
    <Card className="mt-6">
      <CardHeader title="Hook notifications" subtitle={`${notifications.length} notification${notifications.length === 1 ? "" : "s"}`} />
      <div className="space-y-3">
        {notifications.map((notification) => (
          <div key={notification.id} className="flex flex-wrap items-center gap-x-3 gap-y-1 rounded-md border border-border bg-surface px-3 py-2.5 text-sm">
            <span className="font-semibold capitalize text-foreground">{notification.destination_type}</span>
            <StatusBadge status={notification.status} />
            {notification.http_status !== null && <code className="font-mono text-xs text-secondary">HTTP {notification.http_status}</code>}
            <span className="text-xs text-muted">{notification.attempts} attempt{notification.attempts === 1 ? "" : "s"}</span>
            <span className="text-xs text-muted">{formatDateTime(notification.created_at)}</span>
            {notification.error && <p className="basis-full text-xs text-danger">{notification.error}</p>}
          </div>
        ))}
      </div>
    </Card>
  );
}

function AiExplanationCard({ explanation }: { explanation: AiExplanation }) {
  const retryLabel = {
    safe: "Safe to retry",
    caution: "Retry with caution",
    not_recommended: "Retry not recommended",
  }[explanation.retry_recommendation];

  return (
    <Card className="mb-6">
      <CardHeader title="AI explanation" subtitle="Based on redacted event and delivery data" />
      <div className="space-y-4 text-sm">
        <div>
          <p className="text-xs text-muted uppercase mb-1">Summary</p>
          <p className="text-foreground">{explanation.summary}</p>
        </div>
        <div>
          <p className="text-xs text-muted uppercase mb-1">Likely cause</p>
          <p className="text-foreground">{explanation.likely_cause}</p>
        </div>
        <div>
          <p className="text-xs text-muted uppercase mb-1">Evidence</p>
          <ul className="list-disc pl-5 space-y-1 text-secondary">
            {explanation.evidence.map((item) => <li key={item}>{item}</li>)}
          </ul>
        </div>
        <div>
          <p className="text-xs text-muted uppercase mb-1">Recommended actions</p>
          <ul className="list-disc pl-5 space-y-1 text-secondary">
            {explanation.recommended_actions.map((item) => <li key={item}>{item}</li>)}
          </ul>
        </div>
        <p className="text-xs text-muted">Retry recommendation: <span className="text-foreground font-semibold">{retryLabel}</span></p>
      </div>
    </Card>
  );
}

/** Vertical timeline of every delivery attempt for the event. */
function DeliveryTimeline({
  attempts,
}: {
  attempts: import("../types/api").DeliveryAttempt[];
}) {
  if (attempts.length === 0) return null;
  return (
    <Card className="mt-6">
      <CardHeader
        title="Delivery attempts"
        subtitle={`${attempts.length} attempt${attempts.length === 1 ? "" : "s"}`}
      />
      <ol className="relative border-l border-border ml-3 space-y-4">
        {attempts.map((a) => {
          const ok = a.status === "delivered";
          const retry = a.status === "retrying";
          return (
            <li key={a.id} className="ml-6">
              <span
                className={`absolute -left-[9px] flex h-4 w-4 items-center justify-center rounded-full ring-4 ring-card ${
                  ok
                    ? "bg-success"
                    : retry
                      ? "bg-warning"
                      : "bg-danger"
                }`}
              />
              <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
                <span className="text-sm font-semibold text-foreground">
                  #{a.attempt_number}
                </span>
                <StatusBadge status={a.status} />
                {a.http_status !== null && (
                  <code className="text-xs text-secondary font-mono">
                    HTTP {a.http_status}
                  </code>
                )}
                {a.duration_ms !== null && (
                  <span className="text-xs text-muted">{a.duration_ms} ms</span>
                )}
                <span className="text-xs text-muted">
                  {formatDateTime(a.created_at)}
                </span>
              </div>
              {a.error && (
                <p className="mt-1 text-xs text-danger font-mono break-all">
                  {a.error}
                </p>
              )}
              {a.response_body && (
                <details className="mt-2 group">
                  <summary className="text-xs text-muted cursor-pointer hover:text-secondary select-none flex items-center gap-1">
                    <History className="h-3 w-3" /> response body
                  </summary>
                  <pre className="mt-2 bg-surface border border-border rounded-md p-3 text-xs text-secondary font-mono overflow-x-auto max-h-60">
                    {prettyJson(a.response_body)}
                  </pre>
                </details>
              )}
            </li>
          );
        })}
      </ol>
    </Card>
  );
}
