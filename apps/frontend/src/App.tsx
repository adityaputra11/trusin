import {
  createBrowserRouter,
  Navigate,
  RouterProvider,
} from "react-router-dom";
import { Layout } from "./components/layout/Layout";
import { Login } from "./pages/Login";
import { Dashboard } from "./pages/Dashboard";
import { EventDetail } from "./pages/EventDetail";
import { Providers } from "./pages/Providers";
import { Hooks } from "./pages/Hooks";
import { SendWebhook } from "./pages/SendWebhook";
import { Metrics } from "./pages/Metrics";
import { Settings } from "./pages/Settings";
import { Activity } from "./pages/Activity";
import { Users } from "./pages/Users";
import { isLoggedIn } from "./lib/auth";
import { useMe } from "./lib/hooks";
import { FullSpinner } from "./components/ui";

/**
 * Guard for protected routes. Two login modes are supported:
 *   - Basic auth: `terusin.auth` set in sessionStorage → instant pass.
 *   - Google OAuth: a `terusin_session` cookie validated server-side via
 *     /api/auth/me. While that probe is in flight we show a spinner so the
 *     user doesn't see a flash of the login page on a cookie-authed reload.
 */
function Protected({ children }: { children: React.ReactNode }) {
  const me = useMe();
  // Basic auth: synchronous.
  if (isLoggedIn()) return <>{children}</>;
  // Cookie session: probe.
  if (me.isLoading) return <FullSpinner label="Checking session…" />;
  if (me.isSuccess) return <>{children}</>;
  return <Navigate to="/login" replace />;
}

const router = createBrowserRouter([
  { path: "/login", element: <Login /> },
  {
    element: (
      <Protected>
        <Layout />
      </Protected>
    ),
    children: [
      { path: "/", element: <Dashboard /> },
      { path: "/event/:id", element: <EventDetail /> },
      { path: "/providers", element: <Providers /> },
      { path: "/hooks", element: <Hooks /> },
      { path: "/metrics", element: <Metrics /> },
      { path: "/activity", element: <Activity /> },
      { path: "/users", element: <Users /> },
      { path: "/send", element: <SendWebhook /> },
      { path: "/settings", element: <Settings /> },
    ],
  },
  { path: "*", element: <Navigate to="/" replace /> },
]);

export function App() {
  return <RouterProvider router={router} />;
}
