import { createContext, useContext } from "react";
import type { SessionUser } from "./hooks";

/**
 * Lazily-resolved session user, populated by Layout's useMe() probe.
 * Pages read this via `useCurrentUser()` instead of calling useMe() again
 * (which would re-fire the request on every mount).
 *
 * `undefined` means "still loading"; `null` means "no session / Basic auth".
 */
export const UserContext = createContext<SessionUser | null | undefined>(
  undefined,
);

export function useCurrentUser(): SessionUser | null {
  const ctx = useContext(UserContext);
  // During the initial load (Protected still probing) we treat undefined as
  // null so canWrite() / display logic falls back safely.
  return ctx ?? null;
}

export function useCanWrite(): boolean {
  const user = useCurrentUser();
  return user?.role === "admin";
}
