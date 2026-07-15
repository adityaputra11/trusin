import { memo, useCallback, useEffect, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import {
  Inbox,
  Search,
  RotateCw,
  Trash2,
  SlidersHorizontal,
  X,
  AlertCircle,
} from "lucide-react";
import {
  useEvents,
  useEventStream,
  useSources,
  useBulkRetry,
  useBulkDelete,
} from "../lib/hooks";
import type { EventStreamStatus } from "../lib/hooks";
import { useCanWrite } from "../lib/user-context";
import { EndpointBox } from "../components/EndpointBox";
import { EventRow } from "../components/EventRow";
import {
  Card,
  EmptyState,
  FullSpinner,
  Field,
  Input,
  Pagination,
  Select,
  Table,
  TBody,
  TH,
  THead,
  Button,
  Modal,
  ConfirmDialog,
} from "../components/ui";
import type { EventQuery } from "../types/api";

// Module-level so it's not recreated each render.
const STATUS_FILTERS: { value: string; label: string }[] = [
  { value: "all", label: "All statuses" },
  { value: "queued", label: "Queued" },
  { value: "retrying", label: "Retrying" },
  { value: "delivered", label: "Delivered" },
  { value: "failed", label: "Failed" },
];

const STATUS_LABEL: Record<string, string> = Object.fromEntries(
  STATUS_FILTERS.map((s) => [s.value, s.label]),
);

/** Small "Live" / "Reconnecting…" indicator reflecting the SSE stream. Sits
 * next to the endpoint box so users know when real-time updates are paused. */
function LiveBadge({ status }: { status: EventStreamStatus }) {
  if (status === "idle") return null;
  const connected = status === "connected";
  return (
    <span
      className={`inline-flex items-center gap-1.5 text-xs font-medium px-2.5 h-7 rounded-md border self-end mb-6 ${
        connected
          ? "text-success bg-[rgba(34,197,94,.08)] border-[rgba(34,197,94,.25)]"
          : "text-warning bg-[rgba(245,158,11,.08)] border-[rgba(245,158,11,.25)]"
      }`}
      title={connected ? "Connected — events stream live" : "Stream interrupted — reconnecting…"}
    >
      <span
        className={`h-1.5 w-1.5 rounded-full ${
          connected ? "bg-success animate-pulse" : "bg-warning"
        }`}
      />
      {connected ? "Live" : "Reconnecting…"}
    </span>
  );
}

// Isolated + memoized so typing in the search input re-renders only this bar,
// not the 50-row table body beneath it. Search is the one filter that stays
// inline; everything else (status/source/date) lives behind the Filters
// button so the bar stays compact.
const FilterBar = memo(function FilterBar({
  search,
  activeFilterCount,
  isFetching,
  onSearchChange,
  onOpenFilters,
  onRefresh,
}: {
  search: string;
  activeFilterCount: number;
  isFetching: boolean;
  onSearchChange: (v: string) => void;
  onOpenFilters: () => void;
  onRefresh: () => void;
}) {
  return (
    <div className="flex flex-wrap items-center gap-2 p-3.5 border-b border-border bg-[rgba(7,10,8,.32)]">
      <div className="flex-1 min-w-[200px] relative">
        <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-muted pointer-events-none" />
        <Input
          value={search}
          onChange={(e) => onSearchChange(e.target.value)}
          placeholder="Search source, target, body…"
          className="pl-9"
        />
      </div>
      <Button
        variant={activeFilterCount > 0 ? "primary" : "outline"}
        size="md"
        onClick={onOpenFilters}
        className="relative"
      >
        <SlidersHorizontal className="h-4 w-4" />
        Filters
        {activeFilterCount > 0 && (
          <span className="ml-1 inline-flex items-center justify-center min-w-[1.25rem] h-5 px-1 text-[10px] font-semibold rounded-full bg-success text-white">
            {activeFilterCount}
          </span>
        )}
      </Button>
      <Button variant="ghost" size="md" onClick={onRefresh} loading={isFetching}>
        <RotateCw className="h-4 w-4" />
        Refresh
      </Button>
    </div>
  );
});

// Compact clickable chips showing each active filter; clicking the X clears
// just that one filter. Renders below FilterBar when there's at least one.
const ActiveFilterChips = memo(function ActiveFilterChips({
  status,
  source,
  from,
  to,
  onClearStatus,
  onClearSource,
  onClearFrom,
  onClearTo,
}: {
  status: string;
  source: string;
  from: string;
  to: string;
  onClearStatus: () => void;
  onClearSource: () => void;
  onClearFrom: () => void;
  onClearTo: () => void;
}) {
  const chips: { label: string; onClear: () => void }[] = [];
  if (status !== "all")
    chips.push({ label: STATUS_LABEL[status] ?? status, onClear: onClearStatus });
  if (source) chips.push({ label: `Source: ${source}`, onClear: onClearSource });
  if (from) chips.push({ label: `From: ${from}`, onClear: onClearFrom });
  if (to) chips.push({ label: `To: ${to}`, onClear: onClearTo });
  if (chips.length === 0) return null;
  return (
    <div className="flex flex-wrap items-center gap-1.5 px-3 py-2 border-b border-border bg-background/40">
      {chips.map((c, i) => (
        <button
          key={i}
          onClick={c.onClear}
          className="inline-flex items-center gap-1 rounded-full bg-hover border border-border px-2 py-0.5 text-xs text-secondary hover:text-foreground hover:border-border-hover transition-base"
        >
          {c.label}
          <X className="h-3 w-3" />
        </button>
      ))}
    </div>
  );
});

// Modal-based filter form. Uses a working-state pattern: edits inside the
// modal don't commit until Apply is clicked, so the user can tweak multiple
// filters without triggering a fetch on each change.
function FilterModal({
  open,
  onClose,
  initial,
  sources,
  onApply,
  onClear,
}: {
  open: boolean;
  onClose: () => void;
  initial: { status: string; source: string; from: string; to: string };
  sources: string[];
  onApply: (v: { status: string; source: string; from: string; to: string }) => void;
  onClear: () => void;
}) {
  const [status, setStatus] = useState(initial.status);
  const [source, setSource] = useState(initial.source);
  const [from, setFrom] = useState(initial.from);
  const [to, setTo] = useState(initial.to);

  // Re-sync working state every time the modal opens (so re-opening after
  // a cancel resets to the last-applied values, not the cancelled draft).
  useEffect(() => {
    if (open) {
      setStatus(initial.status);
      setSource(initial.source);
      setFrom(initial.from);
      setTo(initial.to);
    }
  }, [open, initial.status, initial.source, initial.from, initial.to]);

  return (
    <Modal
      open={open}
      onClose={onClose}
      title="Filter events"
      description="Refine the list by status, source, or date range."
      footer={
        <>
          <Button variant="ghost" onClick={onClear}>
            Clear all
          </Button>
          <Button
            variant="outline"
            onClick={onClose}
          >
            Cancel
          </Button>
          <Button
            variant="primary"
            onClick={() => {
              onApply({ status, source, from, to });
              onClose();
            }}
          >
            Apply filters
          </Button>
        </>
      }
    >
      <div className="space-y-4">
        <Field label="Status" htmlFor="filter-status">
          <Select
            id="filter-status"
            value={status}
            onChange={(e) => setStatus(e.target.value)}
            className="w-full"
          >
            {STATUS_FILTERS.map((s) => (
              <option key={s.value} value={s.value}>
                {s.label}
              </option>
            ))}
          </Select>
        </Field>
        <Field label="Source" htmlFor="filter-source">
          <Select
            id="filter-source"
            value={source}
            onChange={(e) => setSource(e.target.value)}
            className="w-full"
          >
            <option value="">All sources</option>
            {sources.map((s) => (
              <option key={s} value={s}>
                {s}
              </option>
            ))}
          </Select>
        </Field>
        <div className="grid grid-cols-2 gap-3">
          <Field label="From" htmlFor="filter-from">
            <Input
              id="filter-from"
              type="date"
              value={from}
              onChange={(e) => setFrom(e.target.value)}
              className="w-full"
            />
          </Field>
          <Field label="To" htmlFor="filter-to">
            <Input
              id="filter-to"
              type="date"
              value={to}
              onChange={(e) => setTo(e.target.value)}
              className="w-full"
            />
          </Field>
        </div>
      </div>
    </Modal>
  );
}

export function Dashboard() {
  const navigate = useNavigate();
  const canWrite = useCanWrite();

  // UI state (what the user typed) vs committed state (what we query for).
  // Splitting them lets us debounce the actual fetch without throttling input.
  const [searchInput, setSearchInput] = useState("");
  const [committedSearch, setCommittedSearch] = useState("");
  const [status, setStatus] = useState("all");
  const [source, setSource] = useState("");
  const [from, setFrom] = useState("");
  const [to, setTo] = useState("");
  const [page, setPage] = useState(1);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [filterModalOpen, setFilterModalOpen] = useState(false);

  // Debounce timer in a ref: setting it never triggers a re-render.
  const debounceRef = useRef<number | undefined>(undefined);

  const query: EventQuery = {
    search: committedSearch || undefined,
    status,
    source: source || undefined,
    // Date inputs are YYYY-MM-DD; treat as created_at bounds.
    from: from ? `${from}T00:00:00` : undefined,
    to: to ? `${to}T23:59:59` : undefined,
    page,
    per_page: 10,
  };

  // Live updates via SSE; falls back gracefully if the stream is unavailable.
  const streamStatus = useEventStream(true);
  const { data: sources } = useSources();
  const bulkRetry = useBulkRetry();
  const bulkDelete = useBulkDelete();

  // SSE pushes invalidate the query; keep a slow refetchInterval as a backstop.
  const { data, isLoading, isFetching, isError, refetch } = useEvents(query, {
    refetchInterval: 15000,
  });

  const onSearchChange = useCallback((value: string) => {
    setSearchInput(value);
    setPage(1);
    window.clearTimeout(debounceRef.current);
    debounceRef.current = window.setTimeout(
      () => setCommittedSearch(value.trim()),
      400,
    );
  }, []);

  const resetPage = useCallback(() => setPage(1), []);

  // Filter modal handlers — single apply path, one place that resets the page.
  const applyFilters = useCallback(
    (v: { status: string; source: string; from: string; to: string }) => {
      setStatus(v.status);
      setSource(v.source);
      setFrom(v.from);
      setTo(v.to);
      resetPage();
    },
    [resetPage],
  );

  const clearAllFilters = useCallback(() => {
    setStatus("all");
    setSource("");
    setFrom("");
    setTo("");
    resetPage();
  }, [resetPage]);

  const clearStatus = useCallback(() => { setStatus("all"); resetPage(); }, [resetPage]);
  const clearSource = useCallback(() => { setSource(""); resetPage(); }, [resetPage]);
  const clearFrom = useCallback(() => { setFrom(""); resetPage(); }, [resetPage]);
  const clearTo = useCallback(() => { setTo(""); resetPage(); }, [resetPage]);

  const activeFilterCount =
    (status !== "all" ? 1 : 0) +
    (source ? 1 : 0) +
    (from ? 1 : 0) +
    (to ? 1 : 0);

  const onRefresh = useCallback(() => {
    refetch();
  }, [refetch]);

  // Stable callback → memoized EventRow children only re-render when their
  // specific event object reference changes, not on every poll.
  const onRowClick = useCallback(
    (id: string) => navigate(`/event/${id}`),
    [navigate],
  );

  const events = data?.events ?? [];

  const allVisibleIds = events.map((e) => e.id);
  const allSelected =
    allVisibleIds.length > 0 && allVisibleIds.every((id) => selected.has(id));

  const toggleAll = useCallback(() => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (allSelected) {
        for (const id of allVisibleIds) next.delete(id);
      } else {
        for (const id of allVisibleIds) next.add(id);
      }
      return next;
    });
  }, [allSelected, allVisibleIds]);

  const toggleOne = useCallback((id: string) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }, []);

  const clearSelection = useCallback(() => setSelected(new Set()), []);

  const [confirmDelete, setConfirmDelete] = useState(false);

  const onBulkRetry = useCallback(() => {
    const ids = Array.from(selected);
    if (ids.length === 0) return;
    bulkRetry.mutate(ids, { onSuccess: clearSelection });
  }, [selected, bulkRetry, clearSelection]);

  const onBulkDelete = useCallback(() => {
    if (selected.size === 0) return;
    setConfirmDelete(true);
  }, [selected]);

  const confirmBulkDelete = useCallback(() => {
    const ids = Array.from(selected);
    bulkDelete.mutate(ids, {
      onSuccess: () => {
        clearSelection();
        setConfirmDelete(false);
      },
    });
  }, [selected, bulkDelete, clearSelection]);

  return (
    <div className="space-y-1">
      <div className="flex items-start justify-between gap-4 flex-wrap">
        <div className="flex-1 min-w-0">
          <EndpointBox />
        </div>
        <LiveBadge status={streamStatus} />
      </div>

      <div className="flex items-end justify-between mb-3 mt-1">
        <div><p className="text-[10px] uppercase tracking-[.13em] text-success font-semibold">Event operations</p><h2 className="text-base font-semibold mt-1 text-foreground">Recent deliveries</h2></div>
        <p className="hidden sm:block text-[11px] text-muted">Auto-refreshing every 15 seconds</p>
      </div>
      <Card className="p-0 overflow-hidden">
        <FilterBar
          search={searchInput}
          activeFilterCount={activeFilterCount}
          isFetching={isFetching && !isLoading}
          onSearchChange={onSearchChange}
          onOpenFilters={() => setFilterModalOpen(true)}
          onRefresh={onRefresh}
        />

        <ActiveFilterChips
          status={status}
          source={source}
          from={from}
          to={to}
          onClearStatus={clearStatus}
          onClearSource={clearSource}
          onClearFrom={clearFrom}
          onClearTo={clearTo}
        />

        <FilterModal
          open={filterModalOpen}
          onClose={() => setFilterModalOpen(false)}
          initial={{ status, source, from, to }}
          sources={sources ?? []}
          onApply={applyFilters}
          onClear={clearAllFilters}
        />

        {selected.size > 0 && canWrite && (
          <div className="flex items-center justify-between gap-3 px-4 py-2 bg-hover border-b border-border text-sm">
            <span className="text-secondary">
              {selected.size} selected
            </span>
            <div className="flex items-center gap-2">
              <Button size="sm" variant="success" onClick={onBulkRetry} loading={bulkRetry.isPending}>
                <RotateCw className="h-3.5 w-3.5" /> Retry
              </Button>
              <Button size="sm" variant="danger" onClick={onBulkDelete} loading={bulkDelete.isPending}>
                <Trash2 className="h-3.5 w-3.5" /> Delete
              </Button>
              <Button size="sm" variant="ghost" onClick={clearSelection}>
                Clear
              </Button>
            </div>
          </div>
        )}

        {isError ? (
          <EmptyState
            icon={<AlertCircle className="h-10 w-10" strokeWidth={1.5} />}
            title="Couldn't load events"
            description="The request failed. Check the backend connection and try again."
            action={
              <Button variant="outline" size="sm" onClick={() => refetch()}>
                <RotateCw className="h-4 w-4" /> Retry
              </Button>
            }
          />
        ) : isLoading ? (
          <FullSpinner label="Loading events…" />
        ) : events.length === 0 ? (
          <EmptyState
            icon={<Inbox className="h-10 w-10" strokeWidth={1.5} />}
            title="No events yet"
            description="Webhooks received by the backend will show up here in real time."
          />
        ) : (
          <>
            <Table>
              <THead>
                <TH className="w-8">
                  {canWrite && (
                    <input
                      type="checkbox"
                      checked={allSelected}
                      onChange={toggleAll}
                      className="accent-success cursor-pointer"
                      aria-label="Select all"
                    />
                  )}
                </TH>
                <TH>ID</TH>
                <TH>Source</TH>
                <TH>Status</TH>
                <TH>Target</TH>
                <TH>Retry</TH>
                <TH>Time</TH>
              </THead>
              <TBody>
                {events.map((ev) => (
                  <EventRow
                    key={ev.id}
                    event={ev}
                    onClick={onRowClick}
                    selected={selected.has(ev.id)}
                    onSelect={canWrite ? toggleOne : undefined}
                  />
                ))}
              </TBody>
            </Table>
            <Pagination
              page={data!.page}
              pages={data!.pages}
              total={data!.total}
              onPageChange={setPage}
            />
          </>
        )}
      </Card>

      <ConfirmDialog
        open={confirmDelete}
        onClose={() => setConfirmDelete(false)}
        title="Delete events"
        description={`Delete ${selected.size} selected event(s)? This cannot be undone.`}
        confirmLabel="Delete"
        danger
        loading={bulkDelete.isPending}
        onConfirm={confirmBulkDelete}
      />
    </div>
  );
}
