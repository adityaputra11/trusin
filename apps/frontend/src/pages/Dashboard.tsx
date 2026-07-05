import { memo, useCallback, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import { Inbox, Search, RotateCw, Trash2 } from "lucide-react";
import {
  useEvents,
  useEventStream,
  useSources,
  useBulkRetry,
  useBulkDelete,
} from "../lib/hooks";
import { useCanWrite } from "../lib/user-context";
import { EndpointBox } from "../components/EndpointBox";
import { EventRow } from "../components/EventRow";
import {
  Card,
  EmptyState,
  FullSpinner,
  Input,
  Pagination,
  Select,
  Table,
  TBody,
  TH,
  THead,
  Button,
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

// Isolated + memoized so typing in the search input re-renders only this bar,
// not the 50-row table body beneath it.
const FilterBar = memo(function FilterBar({
  search,
  status,
  source,
  sources,
  from,
  to,
  isFetching,
  onSearchChange,
  onStatusChange,
  onSourceChange,
  onFromChange,
  onToChange,
  onRefresh,
}: {
  search: string;
  status: string;
  source: string;
  sources: string[];
  from: string;
  to: string;
  isFetching: boolean;
  onSearchChange: (v: string) => void;
  onStatusChange: (v: string) => void;
  onSourceChange: (v: string) => void;
  onFromChange: (v: string) => void;
  onToChange: (v: string) => void;
  onRefresh: () => void;
}) {
  return (
    <div className="flex flex-wrap items-center gap-2 p-3 border-b border-border">
      <div className="flex-1 min-w-[200px] relative">
        <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-muted pointer-events-none" />
        <Input
          value={search}
          onChange={(e) => onSearchChange(e.target.value)}
          placeholder="Search source, target, body…"
          className="pl-9"
        />
      </div>
      <Select
        value={status}
        onChange={(e) => onStatusChange(e.target.value)}
        className="w-40"
      >
        {STATUS_FILTERS.map((s) => (
          <option key={s.value} value={s.value}>
            {s.label}
          </option>
        ))}
      </Select>
      <Select
        value={source}
        onChange={(e) => onSourceChange(e.target.value)}
        className="w-40"
      >
        <option value="">All sources</option>
        {sources.map((s) => (
          <option key={s} value={s}>
            {s}
          </option>
        ))}
      </Select>
      <Input
        type="date"
        value={from}
        onChange={(e) => onFromChange(e.target.value)}
        className="w-36"
        title="From date"
      />
      <Input
        type="date"
        value={to}
        onChange={(e) => onToChange(e.target.value)}
        className="w-36"
        title="To date"
      />
      <Button variant="ghost" size="md" onClick={onRefresh} loading={isFetching}>
        <RotateCw className="h-4 w-4" />
        Refresh
      </Button>
    </div>
  );
});

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
    per_page: 50,
  };

  // Live updates via SSE; falls back gracefully if the stream is unavailable.
  useEventStream(true);
  const { data: sources } = useSources();
  const bulkRetry = useBulkRetry();
  const bulkDelete = useBulkDelete();

  // SSE pushes invalidate the query; keep a slow refetchInterval as a backstop.
  const { data, isLoading, isFetching, refetch } = useEvents(query, {
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
  const onStatusChange = useCallback(
    (v: string) => {
      setStatus(v);
      resetPage();
    },
    [resetPage],
  );
  const onSourceChange = useCallback(
    (v: string) => {
      setSource(v);
      resetPage();
    },
    [resetPage],
  );
  const onFromChange = useCallback(
    (v: string) => {
      setFrom(v);
      resetPage();
    },
    [resetPage],
  );
  const onToChange = useCallback(
    (v: string) => {
      setTo(v);
      resetPage();
    },
    [resetPage],
  );

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

  const onBulkRetry = useCallback(() => {
    const ids = Array.from(selected);
    if (ids.length === 0) return;
    bulkRetry.mutate(ids, { onSuccess: clearSelection });
  }, [selected, bulkRetry, clearSelection]);

  const onBulkDelete = useCallback(() => {
    const ids = Array.from(selected);
    if (ids.length === 0) return;
    if (!confirm(`Delete ${ids.length} selected event(s)?`)) return;
    bulkDelete.mutate(ids, { onSuccess: clearSelection });
  }, [selected, bulkDelete, clearSelection]);

  return (
    <div>
      <EndpointBox />

      <Card className="p-0 overflow-hidden">
        <FilterBar
          search={searchInput}
          status={status}
          source={source}
          sources={sources ?? []}
          from={from}
          to={to}
          isFetching={isFetching && !isLoading}
          onSearchChange={onSearchChange}
          onStatusChange={onStatusChange}
          onSourceChange={onSourceChange}
          onFromChange={onFromChange}
          onToChange={onToChange}
          onRefresh={onRefresh}
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

        {isLoading ? (
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
    </div>
  );
}
