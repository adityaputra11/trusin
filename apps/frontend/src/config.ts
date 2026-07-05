// Build-time configuration. Values come from Vite env vars (prefixed VITE_).
// Falls back to undefined → feature is disabled (e.g. captcha widget not shown).

declare global {
  interface Window {
    // Populated by the Turnstile script callback (see Login.tsx).
    onTurnstileSuccess?: (token: string) => void;
    turnstile?: {
      render: (
        el: string | HTMLElement,
        opts: { sitekey: string; callback: (t: string) => void },
      ) => string;
      reset: (id?: string) => void;
      remove: (id: string) => void;
    };
  }
}

/** Cloudflare Turnstile site key (public). When undefined, captcha is off. */
export const TURNSTILE_SITE_KEY: string | undefined = import.meta.env
  .VITE_TURNSTILE_SITE_KEY as string | undefined;
