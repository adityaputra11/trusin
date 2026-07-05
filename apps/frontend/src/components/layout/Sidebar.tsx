import { memo } from "react";
import { NavLink } from "react-router-dom";
import {
  LayoutDashboard,
  Webhook,
  Send,
  Settings2,
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
  { to: "/send", label: "Send", icon: Send, adminOnly: true },
];

export const Sidebar = memo(function Sidebar() {
  const canWrite = useCanWrite();
  const visibleItems = NAV_ITEMS.filter(
    (i) => !i.adminOnly || canWrite,
  );

  return (
    <aside className="w-60 shrink-0 border-r border-border bg-surface flex flex-col">
      <div className="h-14 flex items-center px-5 border-b border-border">
        <img
          src="/icon-terusin.png"
          alt="Terusin"
          className="h-14 w-auto object-contain"
        />
      </div>

      <nav className="flex-1 py-3 px-2 space-y-0.5">
        {visibleItems.map((item) => {
          const Icon = item.icon;
          return (
            <NavLink
              key={item.to}
              to={item.to}
              end={item.end}
              className={({ isActive }) =>
                `flex items-center gap-3 px-3 py-2 rounded-md text-sm font-medium transition-base border-l-[3px] ${
                  isActive
                    ? "bg-hover text-foreground border-success"
                    : "text-[#A1A1A1] border-transparent hover:bg-[#141414] hover:text-foreground"
                }`
              }
            >
              <Icon className="h-[18px] w-[18px]" strokeWidth={1.75} />
              {item.label}
            </NavLink>
          );
        })}
      </nav>

      <div className="p-4 border-t border-border">
        <p className="text-[10px] uppercase tracking-wider text-muted font-medium">
          Webhook Relay
        </p>
        <p className="text-xs text-secondary mt-1">v0.1.0</p>
      </div>
    </aside>
  );
});
