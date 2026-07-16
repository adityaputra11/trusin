import { useEffect, useState } from "react";
import { Outlet, useLocation } from "react-router-dom";
import { MobileNav, Sidebar } from "./Sidebar";
import { TopBar } from "./TopBar";
import { CommandPalette } from "./CommandPalette";
import { useMe } from "../../lib/hooks";
import { UserContext } from "../../lib/user-context";

const TITLES: Record<string, { title: string; subtitle: string }> = {
  "/": { title: "Dashboard", subtitle: "Webhook events & delivery status" },
  "/providers": { title: "Providers", subtitle: "Primary source → target deliveries" },
  "/hooks": { title: "Hooks", subtitle: "Optional provider follow-up deliveries" },
  "/metrics": { title: "Metrics", subtitle: "Throughput, success rate & top sources" },
  "/send": { title: "Send", subtitle: "Send a custom webhook" },
  "/platform": { title: "Platform", subtitle: "Hosted tenant control plane" },
  "/settings": { title: "Settings", subtitle: "Workspace, team, and developer tools" },
};

function titlesFor(pathname: string) {
  if (pathname.startsWith("/event/")) {
    return { title: "Event Detail", subtitle: "Webhook event" };
  }
  if (pathname.startsWith("/settings")) {
    return TITLES["/settings"];
  }
  return TITLES[pathname] ?? { title: "trusin", subtitle: "" };
}

export function Layout() {
  const location = useLocation();
  const { title, subtitle } = titlesFor(location.pathname);
  // Single source of truth for the current session. Children read it via
  // useCurrentUser() / useCanWrite() — no per-page useMe() calls.
  const me = useMe();
  const user = me.data ?? null;
  const [commandOpen, setCommandOpen] = useState(false);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        setCommandOpen((open) => !open);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  return (
    <UserContext.Provider value={user}>
      <div className="flex h-screen overflow-hidden bg-background">
        <Sidebar />
        <div className="flex-1 flex flex-col min-w-0">
          <TopBar title={title} subtitle={subtitle} />
          <main className="flex-1 overflow-y-auto relative pb-20 lg:pb-0">
            <div className="pointer-events-none absolute inset-x-0 top-0 h-64 bg-[radial-gradient(circle_at_60%_0%,rgba(74,222,128,.035),transparent_55%)]" />
            <div className="relative max-w-[1440px] mx-auto p-4 sm:p-6 lg:p-8 xl:p-10">
              <Outlet />
            </div>
          </main>
          <MobileNav />
          <CommandPalette open={commandOpen} onClose={() => setCommandOpen(false)} />
        </div>
      </div>
    </UserContext.Provider>
  );
}
