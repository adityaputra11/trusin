import { memo, useCallback } from "react";
import { useNavigate } from "react-router-dom";
import { Activity, LogOut } from "lucide-react";
import { clearAuth } from "../../lib/auth";
import { api } from "../../lib/api";
import { useCurrentUser } from "../../lib/user-context";
import { Badge } from "../ui";

interface TopBarProps {
  title: string;
  subtitle?: string;
  actions?: React.ReactNode;
}

export const TopBar = memo(function TopBar({
  title,
  subtitle,
  actions,
}: TopBarProps) {
  const navigate = useNavigate();
  const user = useCurrentUser();

  // Display name: Google user → display_name, Basic auth → username,
  // fallback "user".
  const name =
    user?.display_name || user?.username || user?.email?.split("@")[0] || "user";
  const avatar = user?.avatar_url;
  const isAdmin = user?.role === "admin";

  const logout = useCallback(async () => {
    try {
      await api.post("/api/auth/logout");
    } catch {
      /* ignore */
    }
    clearAuth();
    navigate("/login", { replace: true });
  }, [navigate]);

  return (
    <header className="h-[72px] shrink-0 border-b border-border bg-[rgba(10,13,11,.84)] backdrop-blur-xl flex items-center justify-between px-4 sm:px-6 lg:px-8 sticky top-0 z-20">
      <div className="min-w-0 flex items-center gap-3">
        <div className="lg:hidden h-9 w-9 rounded-md border border-border bg-card grid place-items-center text-success"><Activity className="h-4 w-4" /></div>
        <div>
        <h1 className="text-[15px] font-semibold tracking-[-.015em] text-foreground leading-tight truncate">
          {title}
        </h1>
        {subtitle && <p className="text-[11px] text-muted mt-1 truncate">{subtitle}</p>}
        </div>
      </div>
      <div className="flex items-center gap-3">
        {actions}
        <div className="h-7 w-px bg-border" />
        <div className="flex items-center gap-2.5 text-sm text-secondary">
          {avatar ? (
            <img
              src={avatar}
              alt=""
              className="h-8 w-8 rounded-md object-cover border border-border-light"
              referrerPolicy="no-referrer"
            />
          ) : (
            <div className="h-8 w-8 rounded-md bg-hover border border-border-light flex items-center justify-center text-[10px] font-semibold uppercase text-success">
              {name.slice(0, 2)}
            </div>
          )}
          <span className="hidden sm:inline max-w-[140px] truncate">{name}</span>
          {user && (
            <Badge variant={isAdmin ? "success" : "neutral"}>
              {isAdmin ? "admin" : user.role}
            </Badge>
          )}
        </div>
        <button
          onClick={logout}
          className="p-2 rounded-md border border-transparent text-muted hover:text-danger hover:border-[rgba(239,68,68,.2)] hover:bg-[rgba(239,68,68,.07)] transition-base"
          title="Logout"
        >
          <LogOut className="h-[18px] w-[18px]" strokeWidth={1.75} />
        </button>
      </div>
    </header>
  );
});
