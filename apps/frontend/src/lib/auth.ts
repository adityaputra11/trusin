// Auth helpers — store base64(user:pass) in sessionStorage and attach to
// every API request as a Basic auth header. Mirrors the TUI's auth_client
// pattern (apps/tui/src/main.rs).

const STORAGE_KEY = "terusin.auth";

export function getAuthHeader(): string | null {
  return sessionStorage.getItem(STORAGE_KEY);
}

export function setAuth(user: string, password: string): void {
  const b64 = btoa(`${user}:${password}`);
  sessionStorage.setItem(STORAGE_KEY, `Basic ${b64}`);
}

export function clearAuth(): void {
  sessionStorage.removeItem(STORAGE_KEY);
}

export function isLoggedIn(): boolean {
  return !!getAuthHeader();
}

export function decodeUser(): string | null {
  const header = getAuthHeader();
  if (!header) return null;
  try {
    const b64 = header.replace(/^Basic\s+/, "");
    const decoded = atob(b64);
    return decoded.split(":")[0] ?? null;
  } catch {
    return null;
  }
}
