import { memo, useCallback, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import { Inbox, Search, RotateCw } from "lucide-react";
import { useEvents } from "../lib/hooks";
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
  isFetching,
  onSearchChange,
  onStatusChange,
  onRefresh,
}: {
  search: string;
  status: string;
  isFetching: boolean;
  onSearchChange: (v: string) => void;
  onStatusChange: (v: string) => void;
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
        className="w-44"
      >
        {STATUS_FILTERS.map((s) => (
          <option key={s.value} value={s.value}>
            {s.label}
          </option>
        ))}
      </Select>
      <Button variant="ghost" size="md" onClick={onRefresh} loading={isFetching}>
        <RotateCw className="h-4 w-4" />
        Refresh
      </Button>
    </div>
  );
});

export function Dashboard() {
  const navigate = useNavigate();

  // UI state (what the user typed) vs committed state (what we query for).
  // Splitting them lets us debounce the actual fetch without throttling input.
  const [searchInput, setSearchInput] = useState("");
  const [committedSearch, setCommittedSearch] = useState("");
  const [status, setStatus] = useState("all");
  const [page, setPage] = useState(1);

  // Debounce timer in a ref: setting it never triggers a re-render.
  // (Previously this lived in state — two renders per keystroke.)
  const debounceRef = useRef<number | undefined>(undefined);

  const query: EventQuery = {
    search: committedSearch || undefined,
    status,
    page,
    per_page: 50,
  };

  const { data, isLoading, isFetching, refetch } = useEvents(query, {
    refetchInterval: 5000,
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

  const onStatusChange = useCallback((value: string) => {
    setStatus(value);
    setPage(1);
  }, []);

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

  return (
    <div>
      <EndpointBox />

      <Card className="p-0 overflow-hidden">
        <FilterBar
          search={searchInput}
          status={status}
          isFetching={isFetching && !isLoading}
          onSearchChange={onSearchChange}
          onStatusChange={onStatusChange}
          onRefresh={onRefresh}
        />

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
                <TH>ID</TH>
                <TH>Source</TH>
                <TH>Status</TH>
                <TH>Target</TH>
                <TH>Retry</TH>
                <TH>Time</TH>
              </THead>
              <TBody>
                {events.map((ev) => (
                  <EventRow key={ev.id} event={ev} onClick={onRowClick} />
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
