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

  const masked = url ? `${url.slice(0, 24)}${url.length > 24 ? "•".repeat(8) : ""}` : "";

  return (
    <div className="bg-[rgba(34,197,94,.06)] border border-[rgba(34,197,94,.2)] rounded-lg p-4 mb-6 flex items-center gap-3">
      <div className="h-9 w-9 rounded-md bg-[rgba(34,197,94,.15)] text-success flex items-center justify-center shrink-0">
        <Link2 className="h-5 w-5" />
      </div>
      <div className="flex-1 min-w-0">
        <p className="text-xs font-medium text-success uppercase tracking-wide mb-1">
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
          title={show ? "Hide" : "Show"}
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
