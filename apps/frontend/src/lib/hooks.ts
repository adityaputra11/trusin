// React Query hooks for the backend API.

import { useEffect } from "react";
import {
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import { api } from "./api";
import type {
  CreateRuleInput,
  DeliveryAttempt,
  EndpointInfo,
  EventQuery,
  ForwardRule,
  ListEventsResponse,
  Metrics,
  UpdateRuleInput,
  WebhookEvent,
} from "../types/api";

function buildEventsQuery(q: EventQuery): string {
  const p = new URLSearchParams();
  if (q.search) p.set("search", q.search);
  if (q.status && q.status !== "all") p.set("status", q.status);
  if (q.source) p.set("source", q.source);
  if (q.from) p.set("from", q.from);
  if (q.to) p.set("to", q.to);
  if (q.page) p.set("page", String(q.page));
  if (q.per_page) p.set("per_page", String(q.per_page));
  const s = p.toString();
  return s ? `?${s}` : "";
}

export function useEvents(q: EventQuery, opts?: { refetchInterval?: number }) {
  return useQuery<ListEventsResponse>({
    queryKey: ["events", q],
    queryFn: () => api.get<ListEventsResponse>(`/events${buildEventsQuery(q)}`),
    refetchInterval: opts?.refetchInterval,
    placeholderData: (prev) => prev,
  });
}

export function useEvent(id: string | undefined) {
  return useQuery<WebhookEvent>({
    queryKey: ["event", id],
    queryFn: () => api.get<WebhookEvent>(`/events/${id}`),
    enabled: !!id,
  });
}

/** Delivery attempts for the per-event timeline. Polls while the event is
 * still in flight (queued/retrying) so the timeline updates live. */
export function useAttempts(
  eventId: string | undefined,
  eventStatus: string | undefined,
) {
  const inFlight =
    eventStatus === "queued" || eventStatus === "retrying";
  return useQuery<DeliveryAttempt[]>({
    queryKey: ["attempts", eventId],
    queryFn: () => api.get<DeliveryAttempt[]>(`/events/${eventId}/attempts`),
    enabled: !!eventId,
    refetchInterval: inFlight ? 3000 : false,
  });
}

export function useEndpoint() {
  return useQuery<EndpointInfo>({
    queryKey: ["endpoint"],
    queryFn: () => api.get<EndpointInfo>(`/config/endpoint`),
    refetchInterval: 30_000,
  });
}

export interface OAuthStatus {
  enabled: boolean;
}

/** Whether the backend has Google OAuth configured. The login page uses this
 * to decide whether to render the "Continue with Google" button — hides the
 * button entirely when OAuth is disabled so users never hit a 503. */
export function useOAuthStatus() {
  return useQuery<OAuthStatus>({
    queryKey: ["oauth-status"],
    queryFn: () => api.get<OAuthStatus>(`/config/oauth`),
    staleTime: 5 * 60_000,
  });
}

export interface HealthStatus {
  status: string;
}

export function useHealth() {
  return useQuery<HealthStatus>({
    queryKey: ["health"],
    queryFn: () => api.get<HealthStatus>(`/health`),
    refetchInterval: 15_000,
  });
}

export interface SessionUser {
  id: string;
  username: string | null;
  email: string | null;
  display_name: string | null;
  avatar_url: string | null;
  role: string;
  oauth_provider: string | null;
}

export function useMe() {
  return useQuery<SessionUser>({
    queryKey: ["me"],
    queryFn: () => api.get<SessionUser>(`/api/auth/me`),
    retry: false,
    staleTime: 60_000,
  });
}

/** True if the logged-in user can write (create/edit/delete rules, retry,
 * send webhooks). `viewer` is read-only; everything else is admin. */
export function canWrite(role: string | null | undefined): boolean {
  return role === "admin";
}

export function useRules() {
  return useQuery<ForwardRule[]>({
    queryKey: ["rules"],
    queryFn: () => api.get<ForwardRule[]>(`/rules`),
  });
}

export function useRetryEvent() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => api.post(`/events/${id}/retry`),
    onSuccess: (_data, id) => {
      qc.invalidateQueries({ queryKey: ["events"] });
      qc.invalidateQueries({ queryKey: ["event", id] });
    },
  });
}

export function useCreateRule() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (input: CreateRuleInput) =>
      api.post<ForwardRule>(`/rules`, input),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["rules"] }),
  });
}

export function useDeleteRule() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => api.delete(`/rules/${id}`),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["rules"] }),
  });
}

export function useUpdateRule() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, ...input }: UpdateRuleInput & { id: string }) =>
      api.patch<ForwardRule>(`/rules/${id}`, input),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["rules"] }),
  });
}

export function useDeleteEvent() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => api.delete(`/events/${id}`),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["events"] }),
  });
}

/** Distinct sources, for the dashboard source-filter dropdown. */
export function useSources() {
  return useQuery<string[]>({
    queryKey: ["sources"],
    queryFn: () => api.get<string[]>(`/events/sources`),
    staleTime: 60_000,
  });
}

export type MetricsRange = "24h" | "7d" | "30d";

/** Aggregated observability metrics. Polls every 30s. (Backend route is
 * `/stats` to avoid clashing with the `/metrics` SPA route.) */
export function useMetrics(range: MetricsRange) {
  return useQuery<Metrics>({
    queryKey: ["metrics", range],
    queryFn: () => api.get<Metrics>(`/stats?range=${range}`),
    refetchInterval: 30_000,
  });
}

export function useBulkRetry() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (ids: string[]) =>
      api.post<{ enqueued: number }>(`/events/bulk/retry`, { ids }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["events"] }),
  });
}

export function useBulkDelete() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (ids: string[]) =>
      api.post<{ deleted: number }>(`/events/bulk/delete`, { ids }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["events"] }),
  });
}

/**
 * Subscribe to the live SSE event stream and invalidate the events query when
 * a new event arrives, so the dashboard table refreshes without polling.
 * Returns nothing — it's a side-effect hook. Pass `enabled=false` to pause.
 */
export function useEventStream(enabled: boolean) {
  const qc = useQueryClient();
  useEffect(() => {
    if (!enabled) return;
    const es = new EventSource("/events/stream");
    const onEvent = () => {
      // Invalidate all events queries (any filter/page).
      qc.invalidateQueries({ queryKey: ["events"] });
    };
    es.addEventListener("event", onEvent);
    es.onerror = () => {
      // EventSource auto-reconnects; nothing to do here.
    };
    return () => {
      es.removeEventListener("event", onEvent);
      es.close();
    };
  }, [enabled, qc]);
}
