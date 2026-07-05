import { memo, useCallback } from "react";
import { useNavigate } from "react-router-dom";
import { LogOut } from "lucide-react";
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
    <header className="h-14 shrink-0 border-b border-border bg-surface flex items-center justify-between px-8">
      <div className="min-w-0">
        <h1 className="text-base font-semibold text-foreground leading-tight truncate">
          {title}
        </h1>
        {subtitle && <p className="text-xs text-muted truncate">{subtitle}</p>}
      </div>
      <div className="flex items-center gap-3">
        {actions}
        <div className="h-6 w-px bg-border" />
        <div className="flex items-center gap-2 text-sm text-secondary">
          {avatar ? (
            <img
              src={avatar}
              alt=""
              className="h-7 w-7 rounded-full object-cover border border-border-light"
              referrerPolicy="no-referrer"
            />
          ) : (
            <div className="h-7 w-7 rounded-full bg-hover flex items-center justify-center text-xs font-semibold uppercase">
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
          className="p-2 rounded-md text-muted hover:text-danger hover:bg-[rgba(239,68,68,.1)] transition-base"
          title="Logout"
        >
          <LogOut className="h-[18px] w-[18px]" strokeWidth={1.75} />
        </button>
      </div>
    </header>
  );
});
