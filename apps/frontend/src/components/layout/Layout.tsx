import { Outlet, useLocation } from "react-router-dom";
import { Sidebar } from "./Sidebar";
import { TopBar } from "./TopBar";
import { useMe } from "../../lib/hooks";
import { UserContext } from "../../lib/user-context";

const TITLES: Record<string, { title: string; subtitle: string }> = {
  "/": { title: "Dashboard", subtitle: "Webhook events & delivery status" },
  "/providers": { title: "Providers", subtitle: "Source → target mappings" },
  "/hooks": { title: "Hooks", subtitle: "Forwarding rules" },
  "/send": { title: "Send", subtitle: "Send a custom webhook" },
};

function titlesFor(pathname: string) {
  if (pathname.startsWith("/event/")) {
    return { title: "Event Detail", subtitle: "Webhook event" };
  }
  return TITLES[pathname] ?? { title: "Terusin", subtitle: "" };
}

export function Layout() {
  const location = useLocation();
  const { title, subtitle } = titlesFor(location.pathname);
  // Single source of truth for the current session. Children read it via
  // useCurrentUser() / useCanWrite() — no per-page useMe() calls.
  const me = useMe();
  const user = me.data ?? null;

  return (
    <UserContext.Provider value={user}>
      <div className="flex h-screen overflow-hidden bg-background">
        <Sidebar />
        <div className="flex-1 flex flex-col min-w-0">
          <TopBar title={title} subtitle={subtitle} />
          <main className="flex-1 overflow-y-auto">
            <div className="max-w-7xl mx-auto p-8">
              <Outlet />
            </div>
          </main>
        </div>
      </div>
    </UserContext.Provider>
  );
}
