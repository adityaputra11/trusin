import { useEffect, useRef, useState, type FormEvent } from "react";
import { useNavigate, useSearchParams } from "react-router-dom";
import { AlertCircle, Loader2 } from "lucide-react";
import { ApiError, api } from "../lib/api";
import { setAuth } from "../lib/auth";
import { useOAuthStatus } from "../lib/hooks";
import { Button, Input, Field } from "../components/ui";
import { TURNSTILE_SITE_KEY } from "../config";

export function Login() {
  const navigate = useNavigate();
  const [params] = useSearchParams();
  const oauthError = params.get("error");
  const inviteToken = params.get("invite");
  const { data: oauthStatus } = useOAuthStatus();
  const providers = oauthStatus?.providers ?? [];
  const captchaRequired = oauthStatus?.captcha_required ?? false;

  const [user, setUser] = useState("");
  const [password, setPassword] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(
    oauthError ? decodeURIComponent(oauthError) : null,
  );
  const [turnstileToken, setTurnstileToken] = useState<string | null>(null);
  const [captchaError, setCaptchaError] = useState<string | null>(null);
  const [oauthLoading, setOauthLoading] = useState<"google" | "github" | null>(null);
  const turnstileContainerRef = useRef<HTMLDivElement>(null);
  const turnstileWidgetId = useRef<string | null>(null);

  useEffect(() => {
    if (!captchaRequired) return;
    const siteKey = TURNSTILE_SITE_KEY;
    if (!siteKey) {
      setCaptchaError("Sign-in is temporarily unavailable because captcha is not configured.");
      return;
    }
    if (!turnstileContainerRef.current) return;

    const renderWidget = () => {
      if (turnstileWidgetId.current || !window.turnstile || !turnstileContainerRef.current) {
        return Boolean(turnstileWidgetId.current);
      }

      turnstileWidgetId.current = window.turnstile.render(
        turnstileContainerRef.current,
        {
          sitekey: siteKey,
          callback: (token: string) => {
            setTurnstileToken(token);
            setCaptchaError(null);
          },
          "expired-callback": () => {
            setTurnstileToken(null);
            setCaptchaError("Captcha expired. Please complete it again.");
          },
          "error-callback": () => {
            setTurnstileToken(null);
            setCaptchaError("Cloudflare captcha could not load. Disable blockers and try again.");
          },
        },
      );
      return true;
    };

    if (renderWidget()) return;
    const interval = window.setInterval(() => {
      if (renderWidget()) window.clearInterval(interval);
    }, 100);
    const timeout = window.setTimeout(() => {
      if (!turnstileWidgetId.current) {
        setCaptchaError("Cloudflare captcha could not load. Disable blockers and try again.");
      }
    }, 10_000);

    return () => {
      window.clearInterval(interval);
      window.clearTimeout(timeout);
    };
  }, [captchaRequired]);

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
    if (captchaRequired && !turnstileToken) {
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

  const oauth = async (provider: "google" | "github") => {
    if (captchaRequired && !turnstileToken) {
      setError("Please complete the captcha before continuing.");
      return;
    }
    setError(null);
    setOauthLoading(provider);
    try {
      if (captchaRequired) {
        await api.post("/api/auth/captcha", { turnstile_token: turnstileToken }, { noAuth: true });
      }
    } catch (err) {
      const code = err instanceof ApiError ? err.code : undefined;
      setError(code === "captcha_unavailable" ? "Captcha verification is unavailable. Please try again shortly." : "Captcha verification failed. Please try again.");
      resetTurnstile();
      setOauthLoading(null);
      return;
    }
    const query = inviteToken ? `?invite=${encodeURIComponent(inviteToken)}` : "";
    window.location.assign(`/api/auth/${provider}${query}`);
  };

  const captchaBlocked = captchaRequired && (!TURNSTILE_SITE_KEY || !!captchaError || !turnstileToken);

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
          <h1 className="text-3xl font-semibold tracking-tight text-foreground">Sign in or create your workspace</h1>
          <p className="mt-2 max-w-sm text-sm leading-6 text-secondary">
            Your first Google or GitHub sign-in creates a workspace with a 30-day Pro trial. Invited teammates join the workspace they were invited to.
          </p>
        </div>

        {providers.length > 0 && (
          <>
            {providers.includes("google") && <button disabled={captchaBlocked || oauthLoading !== null} onClick={() => oauth("google")} className="mb-2 flex w-full items-center justify-center gap-3 rounded-lg border border-border bg-card px-4 py-3 text-sm font-medium text-foreground transition-base hover:border-border-hover hover:bg-card-secondary disabled:cursor-not-allowed disabled:opacity-50">{oauthLoading === "google" ? <Loader2 className="h-5 w-5 animate-spin" /> : <GoogleIcon />}Continue with Google</button>}
            {providers.includes("github") && <button disabled={captchaBlocked || oauthLoading !== null} onClick={() => oauth("github")} className="mb-1 flex w-full items-center justify-center gap-3 rounded-lg border border-border bg-card px-4 py-3 text-sm font-medium text-foreground transition-base hover:border-border-hover hover:bg-card-secondary disabled:cursor-not-allowed disabled:opacity-50">{oauthLoading === "github" ? <Loader2 className="h-5 w-5 animate-spin" /> : <GitHubIcon />}Continue with GitHub</button>}

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
          {captchaRequired && TURNSTILE_SITE_KEY && (
            <div ref={turnstileContainerRef} className="min-h-[65px]" />
          )}

          {captchaError && (
            <div className="flex items-start gap-2 text-sm text-danger bg-[rgba(239,68,68,.1)] border border-[rgba(239,68,68,.25)] rounded-md p-3">
              <AlertCircle className="h-4 w-4 mt-0.5 shrink-0" />
              <span>{captchaError}</span>
            </div>
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
            disabled={captchaBlocked}
          >
            {loading ? "Signing in…" : "Sign in"}
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

function GitHubIcon() {
  return (
    <svg className="h-5 w-5 fill-current" viewBox="0 0 24 24" aria-hidden="true">
      <path d="M12 2C6.48 2 2 6.58 2 12.23c0 4.52 2.87 8.35 6.84 9.71.5.1.68-.22.68-.49 0-.24-.01-1.05-.01-1.9-2.78.62-3.37-1.2-3.37-1.2-.45-1.19-1.11-1.5-1.11-1.5-.91-.64.06-.63.06-.63 1.01.07 1.54 1.07 1.54 1.07.9 1.57 2.35 1.12 2.92.86.09-.67.35-1.12.64-1.38-2.22-.26-4.56-1.15-4.56-5.12 0-1.13.39-2.05 1.04-2.77-.1-.26-.45-1.31.1-2.73 0 0 .85-.28 2.75 1.06A9.3 9.3 0 0 1 12 6.9c.85 0 1.7.12 2.5.34 1.9-1.34 2.75-1.06 2.75-1.06.55 1.42.2 2.47.1 2.73.65.72 1.04 1.64 1.04 2.77 0 3.98-2.34 4.86-4.57 5.12.36.32.68.93.68 1.88 0 1.36-.01 2.45-.01 2.79 0 .27.18.59.69.49A10.25 10.25 0 0 0 22 12.23C22 6.58 17.52 2 12 2Z" />
    </svg>
  );
}
