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
    window.location.assign("/api/auth/google");
  };

  return (
    <div className="min-h-screen flex items-center justify-center bg-background p-6">
      <div className="w-full max-w-sm">
        <div className="flex flex-col items-center mb-8">
          <img
            src="/terusin-logo.svg"
            alt="Terusin"
            className="h-14 w-auto object-contain mb-5"
          />
          <p className="text-sm text-muted mt-1">Sign in to your webhook relay</p>
        </div>

        {/* Google sign-in — hidden entirely when OAuth is not configured on
            the backend, so users never hit a 503 by clicking it. */}
        {oauthEnabled && (
          <>
            <button
              onClick={google}
              className="w-full flex items-center justify-center gap-3 bg-card hover:bg-card-secondary border border-border hover:border-border-hover rounded-md px-4 py-3 text-sm font-medium text-foreground transition-base mb-4"
            >
              <GoogleIcon />
              Continue with Google
            </button>

            {/* Divider */}
            <div className="flex items-center gap-3 my-4">
              <div className="flex-1 h-px bg-border" />
              <span className="text-xs text-muted uppercase tracking-wider">or</span>
              <div className="flex-1 h-px bg-border" />
            </div>
          </>
        )}

        <form
          onSubmit={submit}
          className="bg-card border border-border rounded-lg p-6 space-y-4 shadow-[0_2px_8px_rgba(0,0,0,.25)]"
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

        <p className="text-center text-xs text-muted mt-6">
          Default: <code className="text-secondary">admin</code> /{" "}
          <code className="text-secondary">change-me-in-production</code>
        </p>
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
