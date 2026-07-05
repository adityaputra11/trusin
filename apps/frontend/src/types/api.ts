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

export interface ForwardRule {
  id: string;
  name: string;
  source_pattern: string;
  target_url: string;
  method: string;
  headers: Record<string, string> | null;
  active: boolean;
  created_at?: string;
}

export interface CreateRuleInput {
  name: string;
  source_pattern?: string;
  target_url: string;
  method?: string;
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
  page?: number;
  per_page?: number;
}

export interface EndpointInfo {
  endpoint: string;
  ngrok: string | null;
}
