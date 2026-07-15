import { useState } from "react";
import { Copy, Check, Eye, EyeOff, Link2 } from "lucide-react";
import { useEndpoint } from "../lib/hooks";
import { Button } from "./ui";

export function EndpointBox() {
  const { data, isLoading } = useEndpoint();
  const [copied, setCopied] = useState(false);
  const [show, setShow] = useState(false);

  const url = data?.ngrok ?? data?.endpoint ?? "";

  const copy = async () => {
    if (!url) return;
    try {
      await navigator.clipboard.writeText(url);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      /* ignore */
    }
  };

  const masked = url ? "••••••••••••••••" : "";

  return (
    <div className="relative overflow-hidden bg-[linear-gradient(100deg,rgba(74,222,128,.075),rgba(13,17,14,.82)_60%)] border border-[rgba(74,222,128,.18)] rounded-lg p-4 mb-6 flex items-center gap-3 shadow-[inset_0_1px_rgba(255,255,255,.02)]">
      <div className="absolute inset-y-0 left-0 w-[3px] bg-success" />
      <div className="h-10 w-10 rounded-md bg-[rgba(74,222,128,.09)] border border-[rgba(74,222,128,.18)] text-success flex items-center justify-center shrink-0">
        <Link2 className="h-5 w-5" />
      </div>
      <div className="flex-1 min-w-0">
        <p className="text-[10px] font-semibold text-success uppercase tracking-[.12em] mb-1.5">
          {data?.ngrok ? "Live Tunnel" : "Endpoint"}
        </p>
        {isLoading ? (
          <div className="h-5 w-64 bg-hover rounded animate-pulse" />
        ) : url ? (
          <code className="text-sm text-foreground font-mono truncate block">
            {show ? url : masked}
          </code>
        ) : (
          <p className="text-sm text-muted">No endpoint configured</p>
        )}
      </div>
      <div className="flex items-center gap-1 shrink-0">
        <button
          onClick={() => setShow((s) => !s)}
          className="p-2 rounded-md text-muted hover:text-foreground hover:bg-hover transition-base"
          type="button"
          title={show ? "Hide endpoint" : "Reveal endpoint"}
          aria-label={show ? "Hide endpoint" : "Reveal endpoint"}
          aria-pressed={show}
        >
          {show ? <EyeOff className="h-4 w-4" /> : <Eye className="h-4 w-4" />}
        </button>
        <Button size="sm" variant="ghost" onClick={copy} disabled={!url}>
          {copied ? (
            <>
              <Check className="h-4 w-4 text-success" /> Copied
            </>
          ) : (
            <>
              <Copy className="h-4 w-4" /> Copy
            </>
          )}
        </Button>
      </div>
    </div>
  );
}
