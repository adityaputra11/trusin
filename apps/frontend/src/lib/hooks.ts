// React Query hooks for the backend API.

import {
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import { toast } from "sonner";
import { api } from "./api";
import type {
  AiExplanation,
  AiStatus,
  CreateRuleInput,
  DeliveryAttempt,
  EventQuery,
  ForwardRule,
  ListAuditResponse,
  ListEventsResponse,
  Metrics,
  OrganizationDomain,
  OrganizationInvite,
  OrganizationSubscription,
  PlatformOrganizationDetail,
  PlatformOrganizationList,
  PlatformOverview,
  UpdateRuleInput,
  WorkspaceUser,
  WebhookEvent,
  WorkspaceDestination,
  HookNotificationDelivery,
  RuleHealth,
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

export function useDestinations() {
  return useQuery<WorkspaceDestination[]>({ queryKey: ["destinations"], queryFn: () => api.get("/api/destinations") });
}

export function useSaveDestination() {
  const qc = useQueryClient();
  return useMutation({ mutationFn: (input: { kind: "slack" | "telegram"; enabled: boolean; config: Record<string, string> }) => api.post<WorkspaceDestination>("/api/destinations", input), onSuccess: () => qc.invalidateQueries({ queryKey: ["destinations"] }) });
}

export function useTestDestination() {
  return useMutation({
    mutationFn: (kind: "slack" | "telegram") => api.post<void>(`/api/destinations/${kind}/test`),
    onSuccess: () => toast.success("Test notification sent"),
    onError: () => toast.error("Could not send the test notification"),
  });
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

export function useHookNotifications(eventId: string | undefined, eventStatus: string | undefined) {
  const inFlight = eventStatus === "queued" || eventStatus === "retrying";
  return useQuery<HookNotificationDelivery[]>({
    queryKey: ["hook-notifications", eventId],
    queryFn: () => api.get<HookNotificationDelivery[]>(`/events/${eventId}/hook-notifications`),
    enabled: !!eventId,
    refetchInterval: inFlight ? 3000 : false,
  });
}

export function useAiStatus() {
  return useQuery<AiStatus>({
    queryKey: ["ai-status"],
    queryFn: () => api.get<AiStatus>("/config/ai"),
    staleTime: 5 * 60_000,
  });
}

export function useExplainEvent() {
  return useMutation({
    mutationFn: (id: string) =>
      api.post<AiExplanation>(`/events/${id}/ai-explanation`),
  });
}

export interface OAuthStatus {
  enabled: boolean;
  providers: string[];
  captcha_required: boolean;
  passkey_enabled: boolean;
}

/** Browser OAuth providers configured by the backend. */
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
  organization_id: string;
  is_platform_operator: boolean;
}

export function useMe() {
  return useQuery<SessionUser>({
    queryKey: ["me"],
    queryFn: () => api.get<SessionUser>(`/api/auth/me`),
    retry: false,
    staleTime: 60_000,
  });
}

// ── API tokens / device pairing ───────────────────────────────────────────

export interface ApiToken {
  id: string;
  name: string;
  last_used_at: string | null;
  created_at: string;
  scopes: string[];
}

export function useTokens() {
  return useQuery<ApiToken[]>({
    queryKey: ["tokens"],
    queryFn: () => api.get<ApiToken[]>(`/api/auth/tokens`),
  });
}

export interface CreateTokenResponse {
  /** The full `ts_…` API key — shown only here, never persisted server-side. */
  token: string;
  token_id: string;
  name: string;
  role: string;
  scopes: string[];
}

/** Mint a new API key bound to the signed-in user (role-scoped). The cleartext
 *  key is returned exactly once; the caller must capture + display it. */
export function useCreateToken() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (name: string) =>
      api.post<CreateTokenResponse>(`/api/auth/tokens`, { name }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["tokens"] }),
  });
}

export function useRevokeToken() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => api.delete(`/api/auth/tokens/${id}`),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["tokens"] });
      toast.success("Token revoked");
    },
  });
}

export function useOrganization() {
  return useQuery<OrganizationSubscription>({
    queryKey: ["organization"],
    queryFn: () => api.get<OrganizationSubscription>("/api/organization"),
    refetchInterval: 30_000,
    staleTime: 30_000,
    placeholderData: (previous) => previous,
  });
}

export function useDomains(enabled = true) {
  return useQuery<OrganizationDomain[]>({
    queryKey: ["domains"],
    queryFn: () => api.get<OrganizationDomain[]>("/api/domains"),
    enabled,
  });
}

export function useCreateDomain() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (hostname: string) =>
      api.post<OrganizationDomain>("/api/domains", { hostname }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["domains"] });
      qc.invalidateQueries({ queryKey: ["organization"] });
      toast.success("Domain added. Configure DNS, then verify it.");
    },
  });
}

export function useVerifyDomain() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => api.post<OrganizationDomain>(`/api/domains/${id}/verify`),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["domains"] });
      qc.invalidateQueries({ queryKey: ["organization"] });
    },
  });
}

export function useDeleteDomain() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => api.delete(`/api/domains/${id}`),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["domains"] });
      qc.invalidateQueries({ queryKey: ["organization"] });
      toast.success("Domain removed");
    },
  });
}

export function usePlatformOverview() {
  return useQuery<PlatformOverview>({
    queryKey: ["platform", "overview"],
    queryFn: () => api.get<PlatformOverview>("/api/platform/overview"),
    refetchInterval: 30_000,
  });
}

export function usePlatformOrganizations(search = "", status = "") {
  const params = new URLSearchParams({ per_page: "50" });
  if (search.trim()) params.set("search", search.trim());
  if (status) params.set("status", status);
  return useQuery<PlatformOrganizationList>({
    queryKey: ["platform", "organizations", search, status],
    queryFn: () => api.get<PlatformOrganizationList>(`/api/platform/organizations?${params}`),
  });
}

export function usePlatformOrganization(id: string | null) {
  return useQuery<PlatformOrganizationDetail>({
    queryKey: ["platform", "organization", id],
    queryFn: () => api.get<PlatformOrganizationDetail>(`/api/platform/organizations/${id}`),
    enabled: !!id,
  });
}

export interface ProvisionOrganizationInput {
  name: string;
  slug: string;
  username: string;
  password: string;
  email?: string;
  subscriber_name?: string;
  billing_contact_name?: string;
  billing_contact_email?: string;
}

export function useProvisionOrganization() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (input: ProvisionOrganizationInput) =>
      api.post("/api/platform/organizations", input),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["platform"] });
      toast.success("Tenant provisioned");
    },
  });
}

export interface UpdatePlatformSubscriptionInput {
  subscriber_name: string;
  billing_contact_name: string;
  billing_contact_email: string;
  plan_code: string;
  subscription_status: string;
}

export function useUpdatePlatformSubscription() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, ...input }: UpdatePlatformSubscriptionInput & { id: string }) =>
      api.patch(`/api/platform/organizations/${id}/subscription`, input),
    onSuccess: (_data, variables) => {
      qc.invalidateQueries({ queryKey: ["platform"] });
      qc.invalidateQueries({ queryKey: ["platform", "organization", variables.id] });
      toast.success("Subscription details updated");
    },
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

export function useRuleHealth() {
  return useQuery<RuleHealth[]>({
    queryKey: ["rule-health"],
    queryFn: () => api.get<RuleHealth[]>("/rules/health"),
    refetchInterval: 30_000,
  });
}

export function useRetryEvent() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => api.post(`/events/${id}/retry`),
    onSuccess: (_data, id) => {
      qc.invalidateQueries({ queryKey: ["events"] });
      qc.invalidateQueries({ queryKey: ["event", id] });
      toast.success("Event re-queued");
    },
  });
}

export function useCreateRule() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (input: CreateRuleInput) =>
      api.post<ForwardRule>(`/rules`, input),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["rules"] });
      toast.success("Rule created");
    },
  });
}

export function useDeleteRule() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => api.delete(`/rules/${id}`),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["rules"] });
      toast.success("Rule deleted");
    },
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
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["events"] });
      toast.success("Event deleted");
    },
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
    onSuccess: (data) => {
      qc.invalidateQueries({ queryKey: ["events"] });
      toast.success(`Re-queued ${data.enqueued} event(s)`);
    },
  });
}

export function useBulkDelete() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (ids: string[]) =>
      api.post<{ deleted: number }>(`/events/bulk/delete`, { ids }),
    onSuccess: (data) => {
      qc.invalidateQueries({ queryKey: ["events"] });
      toast.success(`Deleted ${data.deleted} event(s)`);
    },
  });
}

export function useAudit(page = 1, perPage = 25) {
  return useQuery<ListAuditResponse>({
    queryKey: ["audit", page, perPage],
    queryFn: () => api.get<ListAuditResponse>(`/api/audit?page=${page}&per_page=${perPage}`),
    placeholderData: (prev) => prev,
    refetchInterval: 30_000,
  });
}

export function useUsers() {
  return useQuery<WorkspaceUser[]>({
    queryKey: ["users"],
    queryFn: () => api.get<WorkspaceUser[]>(`/api/users`),
  });
}

export function useUpdateUserRole() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, role }: { id: string; role: "admin" | "viewer" }) =>
      api.patch<WorkspaceUser>(`/api/users/${id}/role`, { role }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["users"] });
      qc.invalidateQueries({ queryKey: ["audit"] });
      qc.invalidateQueries({ queryKey: ["me"] });
      toast.success("User role updated");
    },
  });
}

export function useInvites() {
  return useQuery<OrganizationInvite[]>({
    queryKey: ["invites"],
    queryFn: () => api.get<OrganizationInvite[]>("/api/invites"),
  });
}

export function useCreateInvite() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (input: { email: string; role: "admin" | "viewer" }) =>
      api.post<OrganizationInvite>("/api/invites", input),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["invites"] });
      toast.success("Invitation sent");
    },
  });
}

export function useResendInvite() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => api.post<OrganizationInvite>(`/api/invites/${id}/resend`),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["invites"] });
      toast.success("Invitation resent");
    },
  });
}

export function useRevokeInvite() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => api.delete(`/api/invites/${id}`),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["invites"] });
      toast.success("Invitation revoked");
    },
  });
}
