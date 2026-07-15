import { useEffect, useRef, useState, type FormEvent } from "react";
import { useNavigate, useSearchParams } from "react-router-dom";
import { AlertCircle, Loader2 } from "lucide-react";
import { api } from "../lib/api";
import { setAuth } from "../lib/auth";
import { useOAuthStatus } from "../lib/hooks";
import { Button, Input, Field } from "../components/ui";
import { TURNSTILE_SITE_KEY } from "../config";

export function Login() {
  const navigate = useNavigate();
  const [params] = useSearchParams();
  const oauthError = params.get("error");
  const inviteToken = params.get("invite");
  // Whether the backend has Google OAuth configured. The button + divider
  // are only rendered when enabled, so users never see a broken 503 path.
  const { data: oauthStatus } = useOAuthStatus();
  const oauthEnabled = oauthStatus?.enabled === true;

  const [user, setUser] = useState("");
  const [password, setPassword] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(
    oauthError ? decodeURIComponent(oauthError) : null,
  );
  const [turnstileToken, setTurnstileToken] = useState<string | null>(null);
  const turnstileContainerRef = useRef<HTMLDivElement>(null);
  const turnstileWidgetId = useRef<string | null>(null);

  // Render the Turnstile widget explicitly so we can capture the token via
  // callback and reset it after each attempt. Skipped entirely when no site
  // key is configured (local dev / Turnstile disabled).
  useEffect(() => {
    if (!TURNSTILE_SITE_KEY || !turnstileContainerRef.current) return;
    if (turnstileWidgetId.current) return; // already rendered
    if (!window.turnstile) return; // script still loading; will retry on next effect

    turnstileWidgetId.current = window.turnstile.render(
      turnstileContainerRef.current,
      {
        sitekey: TURNSTILE_SITE_KEY,
        callback: (token: string) => setTurnstileToken(token),
      },
    );
  }, []);

  const resetTurnstile = () => {
    setTurnstileToken(null);
    if (turnstileWidgetId.current && window.turnstile) {
      window.turnstile.reset(turnstileWidgetId.current);
    }
  };

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setError(null);
    // If captcha is enabled but no token yet, refuse to submit.
    if (TURNSTILE_SITE_KEY && !turnstileToken) {
      setError("Please complete the captcha before signing in.");
      return;
    }
    setLoading(true);
    try {
      // POST /api/auth/login verifies the Turnstile token (when configured),
      // then the credentials. Only on 200 do we persist the Basic cred.
      await api.post(
        "/api/auth/login",
        {
          username: user,
          password,
          turnstile_token: turnstileToken,
        },
        { noAuth: true },
      );
      setAuth(user, password);
      navigate("/", { replace: true });
    } catch (err) {
      const status = (err as { status?: number }).status;
      if (status === 400) {
        setError("Captcha verification failed. Please try again.");
        resetTurnstile();
      } else if (status === 429) {
        setError("Too many attempts. Please wait a minute and try again.");
      } else if (status === 401) {
        setError("Invalid username or password.");
        resetTurnstile();
      } else {
        setError(
          err instanceof Error && err.message
            ? `Login failed: ${err.message}`
            : "Login failed. Please try again.",
        );
        resetTurnstile();
      }
    } finally {
      setLoading(false);
    }
  };

  const google = () => {
    // Hand off to the backend, which redirects to Google's consent screen.
    // The backend sets the session cookie after the callback, then bounces
    // us back to "/".
    const query = inviteToken ? `?invite=${encodeURIComponent(inviteToken)}` : "";
    window.location.assign(`/api/auth/google${query}`);
  };

  return (
    <div className="min-h-screen bg-[radial-gradient(circle_at_50%_12%,rgba(74,222,128,.12),transparent_28%),radial-gradient(circle_at_0%_100%,rgba(74,222,128,.05),transparent_32%)] px-5 py-10 sm:flex sm:items-center sm:justify-center">
      <div className="mx-auto w-full max-w-[420px]">
        <div className="mb-7">
          <div className="mb-7 flex items-center gap-3">
            <img
              src="/icon-trusin.png"
              alt=""
              className="h-11 w-11 rounded-xl object-cover shadow-[0_0_28px_rgba(74,222,128,.22)]"
            />
            <span className="text-xl font-semibold tracking-tight text-foreground">trusin</span>
          </div>
          <p className="mb-2 text-xs font-semibold uppercase tracking-[.16em] text-success">Webhook operations</p>
          <h1 className="text-3xl font-semibold tracking-tight text-foreground">Welcome back</h1>
          <p className="mt-2 max-w-sm text-sm leading-6 text-secondary">
            Sign in to monitor, route, and recover every webhook delivery.
          </p>
        </div>

        {/* Google sign-in — hidden entirely when OAuth is not configured on
            the backend, so users never hit a 503 by clicking it. */}
        {oauthEnabled && (
          <>
            <button
              onClick={google}
              className="mb-1 flex w-full items-center justify-center gap-3 rounded-lg border border-border bg-card px-4 py-3 text-sm font-medium text-foreground transition-base hover:border-border-hover hover:bg-card-secondary"
            >
              <GoogleIcon />
              Continue with Google
            </button>

            <div className="my-5 flex items-center gap-3">
              <div className="flex-1 h-px bg-border" />
              <span className="text-[10px] font-medium uppercase tracking-[.16em] text-muted">or use password</span>
              <div className="flex-1 h-px bg-border" />
            </div>
          </>
        )}

        <form
          onSubmit={submit}
          className="space-y-4 rounded-2xl border border-border bg-[rgba(15,20,17,.92)] p-5 shadow-[0_24px_80px_rgba(0,0,0,.38)] backdrop-blur sm:p-7"
        >
          <Field label="Username" htmlFor="username">
            <Input
              id="username"
              value={user}
              onChange={(e) => setUser(e.target.value)}
              placeholder="admin"
              autoFocus
              autoComplete="username"
              required
            />
          </Field>
          <Field label="Password" htmlFor="password">
            <Input
              id="password"
              type="password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              placeholder="••••••••"
              autoComplete="current-password"
              required
            />
          </Field>

          {/* Cloudflare Turnstile widget. Only rendered when a site key is
              configured at build time. */}
          {TURNSTILE_SITE_KEY && (
            <div ref={turnstileContainerRef} className="min-h-[65px]" />
          )}

          {error && (
            <div className="flex items-start gap-2 text-sm text-danger bg-[rgba(239,68,68,.1)] border border-[rgba(239,68,68,.25)] rounded-md p-3">
              <AlertCircle className="h-4 w-4 mt-0.5 shrink-0" />
              <span>{error}</span>
            </div>
          )}

          <Button
            type="submit"
            className="w-full"
            loading={loading}
            disabled={!!TURNSTILE_SITE_KEY && !turnstileToken}
          >
            {loading ? (
              <>
                <Loader2 className="h-4 w-4 animate-spin" /> Signing in…
              </>
            ) : (
              "Sign in"
            )}
          </Button>
        </form>
        <p className="mt-5 text-center text-xs text-muted">Secure access for your workspace.</p>
      </div>
    </div>
  );
}

// Inline Google "G" mark — keeps the design clean without bundling an SVG asset.
function GoogleIcon() {
  return (
    <svg className="h-5 w-5" viewBox="0 0 24 24" aria-hidden="true">
      <path
        fill="#4285F4"
        d="M22.56 12.25c0-.78-.07-1.53-.2-2.25H12v4.26h5.92c-.26 1.37-1.04 2.53-2.21 3.31v2.77h3.57c2.08-1.92 3.28-4.74 3.28-8.09z"
      />
      <path
        fill="#34A853"
        d="M12 23c2.97 0 5.46-.98 7.28-2.66l-3.57-2.77c-.98.66-2.23 1.06-3.71 1.06-2.86 0-5.29-1.93-6.16-4.53H2.18v2.84A10.99 10.99 0 0 0 12 23z"
      />
      <path
        fill="#FBBC05"
        d="M5.84 14.1A6.6 6.6 0 0 1 5.5 12c0-.73.13-1.44.34-2.1V7.07H2.18A11 11 0 0 0 1 12c0 1.77.42 3.45 1.18 4.93l3.66-2.83z"
      />
      <path
        fill="#EA4335"
        d="M12 5.38c1.62 0 3.06.56 4.21 1.64l3.15-3.15C17.45 2.09 14.97 1 12 1A10.99 10.99 0 0 0 2.18 7.07l3.66 2.83C6.71 7.31 9.14 5.38 12 5.38z"
      />
    </svg>
  );
}
