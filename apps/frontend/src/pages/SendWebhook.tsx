import { useState, type FormEvent } from "react";
import { Navigate } from "react-router-dom";
import { Send, CheckCircle2, AlertCircle } from "lucide-react";
import { api } from "../lib/api";
import { Button, Card, CardHeader, Field, Input, Textarea } from "../components/ui";
import { useCanWrite } from "../lib/user-context";

interface SendResult {
  ok: boolean;
  message: string;
  id?: string;
}

const SAMPLE = JSON.stringify(
  {
    event: "payment.success",
    data: { id: "evt_123", amount: 50000, currency: "IDR" },
  },
  null,
  2,
);

export function SendWebhook() {
  const canWrite = useCanWrite();
  const [source, setSource] = useState("");
  const [body, setBody] = useState(SAMPLE);
  const [target, setTarget] = useState("");
  const [loading, setLoading] = useState(false);
  const [result, setResult] = useState<SendResult | null>(null);

  // Viewers have no business sending webhooks — bounce to dashboard.
  // (Placed after all hooks so the rules-of-hooks hold.)
  if (!canWrite) return <Navigate to="/" replace />;

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setResult(null);
    setLoading(true);

    let parsed: unknown = body;
    try {
      parsed = body.trim() ? JSON.parse(body) : {};
    } catch {
      setResult({
        ok: false,
        message: "Body is not valid JSON.",
      });
      setLoading(false);
      return;
    }

    // POST /{source} to the backend via the same-origin proxy. In dev the
    // Vite server forwards arbitrary POST paths to the backend; in prod the
    // `web` binary reverse-proxies non-GET requests through. We deliberately
    // do NOT use endpointInfo.endpoint as the host — that's the public ingest
    // URL (e.g. ngrok), not the local backend reachable from the browser.
    const path = source ? `/${source}` : "/";
    const headers: Record<string, string> = {
      "X-Webhook-Source": source || "unknown",
    };
    if (target.trim()) headers["X-Target-Url"] = target.trim();

    try {
      const res = await api.post<{ id?: string; status?: string }>(path, parsed, {
        noAuth: true,
      });
      setResult({
        ok: true,
        message: `Webhook queued${res.id ? ` (id: ${res.id.slice(0, 8)})` : ""}.`,
        id: res.id,
      });
    } catch (err) {
      setResult({
        ok: false,
        message:
          err instanceof Error ? err.message : "Failed to send webhook.",
      });
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="max-w-2xl">
      <Card>
        <CardHeader
          title="Send a custom webhook"
          subtitle="Submit a JSON payload to the relay as if it came from a provider."
        />
        <form onSubmit={submit} className="space-y-4">
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            <Field
              label="Source"
              htmlFor="source"
              hint="Becomes the URL path: /{source}"
            >
              <Input
                id="source"
                value={source}
                onChange={(e) => setSource(e.target.value)}
                placeholder="midtrans"
              />
            </Field>
            <Field
              label="Override target (optional)"
              htmlFor="target"
              hint="Sent as X-Target-Url header"
            >
              <Input
                id="target"
                value={target}
                onChange={(e) => setTarget(e.target.value)}
                placeholder="https://example.com/incoming"
              />
            </Field>
          </div>
          <Field label="Body (JSON)" htmlFor="body">
            <Textarea
              id="body"
              value={body}
              onChange={(e) => setBody(e.target.value)}
              rows={12}
              spellCheck={false}
            />
          </Field>

          {result && (
            <div
              className={`flex items-start gap-2 text-sm rounded-md p-3 ${
                result.ok
                  ? "text-success bg-[rgba(34,197,94,.1)] border border-[rgba(34,197,94,.25)]"
                  : "text-danger bg-[rgba(239,68,68,.1)] border border-[rgba(239,68,68,.25)]"
              }`}
            >
              {result.ok ? (
                <CheckCircle2 className="h-4 w-4 mt-0.5 shrink-0" />
              ) : (
                <AlertCircle className="h-4 w-4 mt-0.5 shrink-0" />
              )}
              <span>{result.message}</span>
            </div>
          )}

          <Button type="submit" loading={loading} className="w-full">
            <Send className="h-4 w-4" /> Send webhook
          </Button>
        </form>
      </Card>
    </div>
  );
}
