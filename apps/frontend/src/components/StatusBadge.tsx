import { memo } from "react";
import { Badge, type BadgeVariant } from "./ui";
import type { EventStatus } from "../types/api";

const STATUS_MAP: Record<EventStatus, { variant: BadgeVariant; label: string }> = {
  delivered: { variant: "success", label: "Delivered" },
  failed: { variant: "danger", label: "Failed" },
  retrying: { variant: "warning", label: "Retrying" },
  queued: { variant: "info", label: "Queued" },
};

// Memoized: parent EventRow is memoized, this is its only child. Without
// memo, every EventRow re-render would cascade into a new StatusBadge render
// even when status hasn't changed.
export const StatusBadge = memo(function StatusBadge({
  status,
}: {
  status: EventStatus;
}) {
  const cfg = STATUS_MAP[status] ?? {
    variant: "neutral" as BadgeVariant,
    label: status,
  };
  return (
    <Badge variant={cfg.variant}>
      <span className="h-1.5 w-1.5 rounded-full bg-current" />
      {cfg.label}
    </Badge>
  );
});
