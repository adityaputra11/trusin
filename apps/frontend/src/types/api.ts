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
}

/** Partial update for PATCH /rules/:id. All fields optional. */
export interface UpdateRuleInput {
  name?: string;
  source_pattern?: string;
  target_url?: string;
  method?: string;
  headers?: Record<string, string>;
  active?: boolean;
  signing_secret?: string;
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

export interface EndpointInfo {
  endpoint: string;
  ngrok: string | null;
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
