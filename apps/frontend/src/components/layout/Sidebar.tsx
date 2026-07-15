import { memo } from "react";
import { NavLink } from "react-router-dom";
import {
  LayoutDashboard,
  Webhook,
  Send,
  Settings2,
  BarChart3,
  Settings,
  type LucideIcon,
} from "lucide-react";
import { useCanWrite } from "../../lib/user-context";

interface NavItem {
  to: string;
  label: string;
  icon: LucideIcon;
  end?: boolean;
  adminOnly?: boolean;
}

const NAV_ITEMS: NavItem[] = [
  { to: "/", label: "Dashboard", icon: LayoutDashboard, end: true },
  { to: "/providers", label: "Providers", icon: Settings2 },
  { to: "/hooks", label: "Hooks", icon: Webhook },
  { to: "/metrics", label: "Metrics", icon: BarChart3 },
  { to: "/send", label: "Send", icon: Send, adminOnly: true },
  { to: "/settings", label: "Settings", icon: Settings },
];

export const Sidebar = memo(function Sidebar() {
  const canWrite = useCanWrite();
  const visibleItems = NAV_ITEMS.filter(
    (i) => !i.adminOnly || canWrite,
  );

  return (
    <aside className="hidden lg:flex w-[248px] shrink-0 border-r border-border bg-[rgba(10,13,11,.96)] flex-col relative">
      <div className="absolute inset-y-0 right-0 w-px bg-gradient-to-b from-transparent via-[rgba(74,222,128,.18)] to-transparent pointer-events-none" />
      <div className="h-[72px] flex items-center px-5 border-b border-border">
        <img
          src="/icon-trusin.png"
          alt="trusin"
          className="h-10 w-10 object-contain"
        />
        <span className="ml-3 text-lg font-semibold tracking-tight text-foreground">trusin</span>
        <span className="ml-auto text-[9px] font-semibold tracking-[.14em] text-success border border-[rgba(74,222,128,.2)] bg-[rgba(74,222,128,.06)] px-2 py-1 rounded-full">CORE</span>
      </div>

      <div className="px-5 pt-6 pb-2 text-[10px] font-semibold tracking-[.14em] text-muted uppercase">Workspace</div>
      <nav className="flex-1 py-1 px-3 space-y-1">
        {visibleItems.map((item) => {
          const Icon = item.icon;
          return (
            <NavLink
              key={item.to}
              to={item.to}
              end={item.end}
              className={({ isActive }) =>
                `group flex items-center gap-3 px-3 py-2.5 rounded-md text-[13px] font-medium transition-base border ${
                  isActive
                    ? "bg-[linear-gradient(90deg,rgba(74,222,128,.1),rgba(74,222,128,.025))] text-foreground border-[rgba(74,222,128,.22)] shadow-[inset_3px_0_0_#4ade80]"
                    : "text-secondary border-transparent hover:bg-hover hover:text-foreground hover:border-border"
                }`
              }
            >
              <Icon className="h-[17px] w-[17px] text-muted group-hover:text-success transition-colors" strokeWidth={1.75} />
              {item.label}
            </NavLink>
          );
        })}
      </nav>

      <div className="p-4 border-t border-border">
        <div className="rounded-md border border-border bg-card p-3 flex items-center gap-3">
          <span className="relative flex h-2 w-2"><span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-success opacity-50"/><span className="relative inline-flex rounded-full h-2 w-2 bg-success"/></span>
          <div><p className="text-[11px] text-foreground font-medium">System operational</p><p className="text-[10px] text-muted mt-0.5">trusin core · v0.1.0</p></div>
        </div>
      </div>
    </aside>
  );
});

export const MobileNav = memo(function MobileNav() {
  const canWrite = useCanWrite();
  const visibleItems = NAV_ITEMS.filter((i) => !i.adminOnly || canWrite);

  return (
    <nav className="lg:hidden fixed bottom-0 inset-x-0 z-40 border-t border-border bg-[rgba(8,11,9,.94)] backdrop-blur-xl px-2 pb-[max(.5rem,env(safe-area-inset-bottom))] pt-2">
      <div className="flex items-center justify-around max-w-xl mx-auto">
        {visibleItems.map((item) => {
          const Icon = item.icon;
          return (
            <NavLink
              key={item.to}
              to={item.to}
              end={item.end}
              aria-label={item.label}
              className={({ isActive }) => `min-w-11 h-11 px-2 rounded-md flex flex-col items-center justify-center gap-1 text-[9px] font-medium transition-base ${isActive ? "text-success bg-[rgba(74,222,128,.08)]" : "text-muted hover:text-foreground"}`}
            >
              <Icon className="h-4 w-4" strokeWidth={1.8} />
              <span className="max-w-[48px] truncate">{item.label}</span>
            </NavLink>
          );
        })}
      </div>
    </nav>
  );
});
