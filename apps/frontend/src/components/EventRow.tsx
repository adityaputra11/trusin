import { memo } from "react";
import { StatusBadge } from "./StatusBadge";
import { TD, TR } from "./ui";
import { formatRelative, shortId } from "../lib/format";
import type { WebhookEvent } from "../types/api";

interface EventRowProps {
  event: WebhookEvent;
  onClick: (id: string) => void;
  selected?: boolean;
  onSelect?: (id: string) => void;
}

// Memoized so the 50 rows in the dashboard table only re-render when the
// specific event object changes (referentially), not on every parent render
// (e.g. typing in the search box) or every poll when data is unchanged.
export const EventRow = memo(function EventRow({
  event,
  onClick,
  selected,
  onSelect,
}: EventRowProps) {
  return (
    <TR onClick={() => onClick(event.id)} className={selected ? "bg-hover" : ""}>
      <TD onClick={(e) => e.stopPropagation()}>
        {onSelect && (
          <input
            type="checkbox"
            checked={!!selected}
            onChange={() => onSelect(event.id)}
            className="accent-success cursor-pointer"
            aria-label={`Select event ${event.id}`}
          />
        )}
      </TD>
      <TD>
        <code className="text-xs text-secondary font-mono">
          {shortId(event.id)}
        </code>
      </TD>
      <TD>
        <span className="font-medium text-foreground">{event.source}</span>
      </TD>
      <TD>
        <StatusBadge status={event.status} />
      </TD>
      <TD>
        <code className="text-xs text-muted font-mono truncate max-w-[200px] inline-block align-bottom">
          {event.target_url || "—"}
        </code>
      </TD>
      <TD>
        <span className="text-xs text-secondary">
          {event.retry_count}/{event.max_retries}
        </span>
      </TD>
      <TD>
        <span className="text-xs text-muted">
          {formatRelative(event.created_at)}
        </span>
      </TD>
    </TR>
  );
});
