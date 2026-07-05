// API client. All calls go through here so Basic auth is attached uniformly.
// In dev, Vite proxies /events, /rules, /config, /health to the backend on
// :3011. In prod (embedded in web binary), same-origin calls are forwarded
// by the web app's catch-all, OR the backend exposes CORS for the web origin.

import { clearAuth, getAuthHeader } from "./auth";

export class ApiError extends Error {
  constructor(
    public status: number,
    message: string,
  ) {
    super(message);
    this.name = "ApiError";
  }
}

interface RequestOptions {
  method?: string;
  body?: unknown;
  signal?: AbortSignal;
  // When true, do not attach auth (used by login probe before creds are stored).
  noAuth?: boolean;
  // Override base — used by the Send Webhook page which posts to /{source}
  // on the backend directly.
  baseUrl?: string;
}

const JSON_CONTENT_HEADERS: Record<string, string> = {
  "Content-Type": "application/json",
};

async function request<T>(path: string, opts: RequestOptions = {}): Promise<T> {
  const headers: Record<string, string> = {
    ...(opts.body !== undefined ? JSON_CONTENT_HEADERS : {}),
  };

  if (!opts.noAuth) {
    const auth = getAuthHeader();
    if (auth) headers["Authorization"] = auth;
  }

  const base = opts.baseUrl ?? "";
  const url = path.startsWith("http") ? path : `${base}${path}`;

  const res = await fetch(url, {
    method: opts.method ?? "GET",
    headers,
    body: opts.body !== undefined ? JSON.stringify(opts.body) : undefined,
    signal: opts.signal,
    // Send the session cookie (Google OAuth) alongside Basic auth header.
    credentials: "include",
  });

  if (res.status === 401) {
    clearAuth();
    // Soft-redirect to login; avoids circular import with router.
    if (typeof window !== "undefined" && window.location.pathname !== "/login") {
      window.location.assign("/login");
    }
    throw new ApiError(401, "Unauthorized");
  }

  if (!res.ok) {
    let msg = `${res.status} ${res.statusText}`;
    try {
      const text = await res.text();
      if (text) msg = text;
    } catch {
      /* ignore */
    }
    throw new ApiError(res.status, msg);
  }

  // Some endpoints return empty bodies (DELETE, retry).
  if (res.status === 204) return undefined as T;
  const ct = res.headers.get("content-type") ?? "";
  if (!ct.includes("application/json")) return undefined as T;
  return (await res.json()) as T;
}

export const api = {
  get: <T>(path: string, opts?: RequestOptions) =>
    request<T>(path, { ...opts, method: "GET" }),
  post: <T>(path: string, body?: unknown, opts?: RequestOptions) =>
    request<T>(path, { ...opts, method: "POST", body }),
  delete: <T>(path: string, opts?: RequestOptions) =>
    request<T>(path, { ...opts, method: "DELETE" }),
  raw: request,
};
