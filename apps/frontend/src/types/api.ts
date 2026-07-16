// API types mirroring the backend Rust structs in apps/backend/src/main.rs.

export type EventStatus = "queued" | "retrying" | "delivered" | "failed";

export interface WebhookEvent {
  id: string;
  source: string;
  headers: Record<string, string>;
  body: unknown;
  status: EventStatus;
  target_url: string;
  retry_count: number;
  max_retries: number;
  created_at: string; // "YYYY-MM-DDTHH:MM:SS"
  response_status: number | null;
  response_headers: Record<string, string> | null;
  response_body: string | null;
}

/** One outbound delivery attempt (retry timeline entry). */
export interface DeliveryAttempt {
  id: string;
  event_id: string;
  attempt_number: number;
  status: "delivered" | "failed" | "retrying" | "network_error";
  http_status: number | null;
  response_headers: Record<string, string> | null;
  response_body: string | null;
  error: string | null;
  duration_ms: number | null;
  created_at: string;
}

export interface ForwardRule {
  id: string;
  name: string;
  source_pattern: string;
  target_url: string;
  method: string;
  headers: Record<string, string> | null;
  active: boolean;
  rule_kind: "provider" | "hook" | string;
  provider_id: string | null;
  trigger_on: "success" | "failure" | string;
  destination_type: "webhook" | "slack" | "telegram" | "email" | string;
  ingest_hostname: string | null;
  signing_secret?: string | null;
  created_at?: string;
}

export interface CreateRuleInput {
  name: string;
  source_pattern?: string;
  target_url: string;
  method?: string;
  headers?: Record<string, string>;
  signing_secret?: string;
  rule_kind?: "provider" | "hook";
  provider_id?: string;
  trigger_on?: "success" | "failure";
  destination_type?: "webhook" | "slack" | "telegram" | "email";
  destination_config?: Record<string, string>;
  ingest_hostname?: string;
}

/** Partial update for PATCH /rules/:id. All fields optional. */
export interface UpdateRuleInput {
  name?: string;
  source_pattern?: string;
  target_url?: string;
  method?: string;
  headers?: Record<string, string>;
  active?: boolean;
  trigger_on?: "success" | "failure";
  signing_secret?: string;
  destination_type?: "webhook" | "slack" | "telegram" | "email";
  destination_config?: Record<string, string>;
  ingest_hostname?: string;
}

export interface ListEventsResponse {
  events: WebhookEvent[];
  total: number;
  page: number;
  per_page: number;
  pages: number;
}

export interface EventQuery {
  search?: string;
  status?: string;
  source?: string;
  from?: string;
  to?: string;
  page?: number;
  per_page?: number;
}

export interface MetricsBucket {
  bucket: string; // "YYYY-MM-DDTHH:MM:SS"
  count: number;
}

export interface MetricsTopItem {
  source?: string;
  target?: string;
  count: number;
}

export interface Metrics {
  range_hours: number;
  total: number;
  delivered: number;
  failed: number;
  success_rate: number;
  queue_depth: number;
  retry_depth: number;
  series: MetricsBucket[];
  top_sources: MetricsTopItem[];
  top_targets: MetricsTopItem[];
}

export interface AuditEntry {
  id: string;
  actor_user_id: string | null;
  actor_email: string | null;
  action: string;
  resource_type: string;
  resource_id: string | null;
  metadata: Record<string, unknown>;
  created_at: string;
}

export interface ListAuditResponse {
  entries: AuditEntry[];
  total: number;
  page: number;
  per_page: number;
  pages: number;
}

export interface WorkspaceUser {
  id: string;
  username: string | null;
  email: string | null;
  display_name: string | null;
  avatar_url: string | null;
  oauth_provider: string | null;
  role: "admin" | "viewer" | string;
  created_at: string;
}

export interface OrganizationInvite {
  id: string;
  email: string;
  role: "admin" | "viewer" | string;
  expires_at: string;
  accepted_at: string | null;
  revoked_at: string | null;
  created_at: string;
}

export interface OrganizationDomain {
  id: string;
  hostname: string;
  verification_token: string;
  status: "pending" | "verified" | "active" | "failed";
  verified_at: string | null;
  created_at: string;
}

export interface OrganizationSubscription {
  organization: {
    id: string;
    name: string;
    slug: string;
    plan_code: string;
    subscription_status: string;
    billing_period_start: string;
    billing_period_end: string;
  };
  hosted: boolean;
  ingest_canonical_host: string;
  ingest_url: string;
  usage: {
    period_start: string;
    period_end: string;
    events_accepted: number;
    domains: number;
    providers: number;
    api_keys: number;
    users: number;
  };
  limits: {
    events: number | null;
    domains: number | null;
    providers: number | null;
    api_keys: number | null;
    users: number | null;
    retention_days: number | null;
  };
}

export interface PlatformOverview {
  period_start: string;
  organizations: number;
  active_organizations: number;
  accepted_events: number;
  queued_events: number;
  retrying_events: number;
  failed_events_24h: number;
  active_domains: number;
}

export interface PlatformOrganization {
  id: string;
  name: string;
  slug: string;
  subscriber_name: string;
  billing_contact_name: string;
  billing_contact_email: string;
  plan_code: string;
  subscription_status: string;
  billing_period_start: string;
  billing_period_end: string;
  created_at: string;
  events_accepted: number;
  active_domains: number;
  active_api_keys: number;
  queued_events: number;
  retrying_events: number;
  last_activity_at: string | null;
}

export interface PlatformOrganizationDetail {
  organization: PlatformOrganization;
  health_24h: { total: number; delivered: number; failed: number; in_flight: number };
  users: Array<{ id: string; username: string | null; email: string | null; role: string; created_at: string }>;
  domains: Array<{ id: string; hostname: string; status: string; verified_at: string | null }>;
  api_keys: Array<{ id: string; name: string; scopes: string[]; last_used_at: string | null; created_at: string }>;
}

export interface PlatformOrganizationList {
  organizations: PlatformOrganization[];
  total: number;
  page: number;
  per_page: number;
  pages: number;
}
