import { memo } from "react";
import { Badge, type BadgeVariant } from "./ui";
import type { EventStatus } from "../types/api";

const STATUS_MAP: Partial<Record<string, { variant: BadgeVariant; label: string }>> = {
  delivered: { variant: "success", label: "Delivered" },
  failed: { variant: "danger", label: "Failed" },
  retrying: { variant: "warning", label: "Retrying" },
  queued: { variant: "info", label: "Queued" },
  network_error: { variant: "danger", label: "Network error" },
};

// Memoized: parent EventRow is memoized, this is its only child. Without
// memo, every EventRow re-render would cascade into a new StatusBadge render
// even when status hasn't changed.
// Accepts any status string so it can render delivery-attempt statuses
// (e.g. "network_error") in addition to the core event statuses.
export const StatusBadge = memo(function StatusBadge({
  status,
}: {
  status: EventStatus | string;
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
