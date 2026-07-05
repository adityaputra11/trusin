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
} from "lucide-react";
import { useEvent, useRetryEvent, useDeleteEvent, useAttempts } from "../lib/hooks";
import { useCanWrite } from "../lib/user-context";
import { StatusBadge } from "../components/StatusBadge";
import {
  Button,
  Card,
  CardHeader,
  FullSpinner,
  EmptyState,
} from "../components/ui";
import { formatDateTime, prettyJson } from "../lib/format";
import type { EventStatus } from "../types/api";

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
        {canWrite && (
        <div className="flex items-center gap-2">
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
            onClick={() => {
              if (confirm("Delete this event permanently?")) {
                deleteEvent.mutate(ev.id, {
                  onSuccess: () => navigate("/"),
                });
              }
            }}
          >
            <Trash2 className="h-4 w-4" /> Delete
          </Button>
        </div>
        )}
      </div>

      {retry.isSuccess && (
        <div className="mb-4 text-sm text-success bg-[rgba(34,197,94,.1)] border border-[rgba(34,197,94,.25)] rounded-md p-3">
          Retry triggered — event re-queued for delivery.
        </div>
      )}

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

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        <Card>
          <CardHeader
            title="Request"
            subtitle="Incoming webhook payload"
          />
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
    </div>
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
