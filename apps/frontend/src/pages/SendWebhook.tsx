import { useState, type FormEvent } from "react";
import { Navigate } from "react-router-dom";
import { Send, CheckCircle2, AlertCircle } from "lucide-react";
import { api } from "../lib/api";
import { useRules } from "../lib/hooks";
import { Button, Card, CardHeader, Field, Input, Select, Textarea } from "../components/ui";
import { useCanWrite } from "../lib/user-context";

interface SendResult {
  ok: boolean;
  message: string;
  id?: string;
}

const CUSTOM_PROVIDER = "__custom__";

const SAMPLE = JSON.stringify(
  {
    event: "payment.success",
    data: { id: "evt_123", amount: 50000, currency: "IDR" },
  },
  null,
  2,
);

function validateTarget(value: string): string | null {
  if (!value.trim()) return null;
  try {
    const url = new URL(value.trim());
    if (url.protocol !== "http:" && url.protocol !== "https:") {
      return "Target must use http or https.";
    }
    if (url.username || url.password) {
      return "Target must not include credentials in the URL.";
    }
    return null;
  } catch {
    return "Target must be a valid URL.";
  }
}

export function SendWebhook() {
  const canWrite = useCanWrite();
  const { data: rules, isLoading: providersLoading, isError: providersError } = useRules();
  const [providerId, setProviderId] = useState(CUSTOM_PROVIDER);
  const [source, setSource] = useState("");
  const [target, setTarget] = useState("");
  const [body, setBody] = useState(SAMPLE);
  const [loading, setLoading] = useState(false);
  const [result, setResult] = useState<SendResult | null>(null);

  const providers = (rules ?? []).filter(
    (rule) => rule.rule_kind === "provider" && rule.active && rule.target_url.trim(),
  );
  const selectedProvider = providers.find((rule) => rule.id === providerId);
  const providerSource = selectedProvider
    ? selectedProvider.source_pattern === "*"
      ? selectedProvider.name
      : selectedProvider.source_pattern
    : "";
  const isCustom = !selectedProvider;

  if (!canWrite) return <Navigate to="/" replace />;

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setResult(null);

    let parsed: unknown;
    try {
      parsed = body.trim() ? JSON.parse(body) : {};
    } catch {
      setResult({ ok: false, message: "Body is not valid JSON." });
      return;
    }

    const customTargetError = isCustom ? validateTarget(target) : null;
    if (customTargetError) {
      setResult({ ok: false, message: customTargetError });
      return;
    }

    setLoading(true);
    try {
      const res = await api.post<{ id?: string; status?: string }>("/api/send", {
        provider_id: selectedProvider?.id,
        source: isCustom ? source.trim() || undefined : undefined,
        target_url: isCustom ? target.trim() || undefined : undefined,
        body: parsed,
      });
      setResult({
        ok: true,
        message: `Webhook queued${res.id ? ` (id: ${res.id.slice(0, 8)})` : ""}.`,
        id: res.id,
      });
    } catch (err) {
      setResult({
        ok: false,
        message: err instanceof Error ? err.message : "Failed to send webhook.",
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
          subtitle="Send a JSON payload through an existing provider or a validated custom target."
        />
        <form onSubmit={submit} className="space-y-4">
          <Field
            label="Provider"
            htmlFor="provider"
            hint={
              providersError
                ? "Providers could not be loaded. Custom mode is still available."
                : "Choose a configured provider, or use Custom for manual routing."
            }
          >
            <Select
              id="provider"
              value={selectedProvider?.id ?? CUSTOM_PROVIDER}
              onChange={(e) => setProviderId(e.target.value)}
              disabled={providersLoading}
            >
              <option value={CUSTOM_PROVIDER}>
                {providersLoading ? "Loading providers…" : "Custom / manual"}
              </option>
              {providers.map((provider) => (
                <option key={provider.id} value={provider.id}>
                  {provider.name} · {provider.source_pattern}
                </option>
              ))}
            </Select>
          </Field>

          {selectedProvider ? (
            <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
              <Field label="Source" htmlFor="provider-source" hint="Resolved from the selected provider.">
                <Input id="provider-source" value={providerSource} readOnly aria-readonly="true" />
              </Field>
              <Field label="Target" htmlFor="provider-target" hint="Resolved from the provider rule.">
                <Input id="provider-target" value={selectedProvider.target_url} readOnly aria-readonly="true" />
              </Field>
            </div>
          ) : (
            <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
              <Field label="Source" htmlFor="source" hint="Optional source label. Empty uses the default source.">
                <Input
                  id="source"
                  value={source}
                  onChange={(e) => setSource(e.target.value)}
                  placeholder="midtrans"
                  maxLength={255}
                />
              </Field>
              <Field
                label="Target (optional)"
                htmlFor="target"
                hint="Leave empty to use the configured default target."
              >
                <Input
                  id="target"
                  type="url"
                  value={target}
                  onChange={(e) => setTarget(e.target.value)}
                  placeholder="https://example.com/incoming"
                />
              </Field>
            </div>
          )}

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
