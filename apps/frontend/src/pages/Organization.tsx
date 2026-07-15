import { useState } from "react";
import { ExternalLink, Globe2, KeyRound, Trash2 } from "lucide-react";
import { useNavigate } from "react-router-dom";
import { Badge, Button, Card, CardHeader, ConfirmDialog, Input } from "../components/ui";
import {
  useCreateDomain,
  useDeleteDomain,
  useDomains,
  useOrganization,
  useVerifyDomain,
} from "../lib/hooks";
import { useCanWrite } from "../lib/user-context";

function LimitBar({ label, used, limit }: { label: string; used: number; limit: number | null }) {
  const unlimited = limit === null;
  const percent = unlimited ? 0 : Math.min(100, (used / Math.max(limit, 1)) * 100);
  return (
    <div className="space-y-1.5">
      <div className="flex justify-between gap-3 text-xs">
        <span className="text-secondary">{label}</span>
        <span className="text-muted">{unlimited ? `${used} · Unlimited` : `${used.toLocaleString()} / ${limit.toLocaleString()}`}</span>
      </div>
      <div className="h-1.5 rounded-full bg-surface overflow-hidden">
        <div className={`h-full rounded-full ${percent >= 90 ? "bg-warning" : "bg-success"}`} style={{ width: unlimited ? "8%" : `${percent}%` }} />
      </div>
    </div>
  );
}

export function Organization() {
  const navigate = useNavigate();
  const organization = useOrganization();
  const canWrite = useCanWrite();
  const domains = useDomains(canWrite);
  const createDomain = useCreateDomain();
  const verifyDomain = useVerifyDomain();
  const deleteDomain = useDeleteDomain();
  const [hostname, setHostname] = useState("");
  const [domainPendingRemoval, setDomainPendingRemoval] = useState<string | null>(null);
  const data = organization.data;

  const addDomain = async () => {
    const value = hostname.trim();
    if (!value) return;
    await createDomain.mutateAsync(value);
    setHostname("");
  };

  if (organization.isLoading) return <p className="text-sm text-muted">Loading organization…</p>;
  if (!data) {
    return (
      <Card className="max-w-xl">
        <CardHeader
          title="Organization details are unavailable"
          subtitle="We could not load your workspace details. Please try again."
          action={<Button size="sm" variant="outline" onClick={() => organization.refetch()}>Retry</Button>}
        />
      </Card>
    );
  }

  const { usage, limits } = data;
  return (
    <div className="max-w-5xl space-y-6">
      <Card>
        <CardHeader
          title={data.organization.name}
          subtitle={data.hosted ? "Hosted organization with Free-plan entitlements" : "Self-hosted organization — unlimited entitlements"}
          action={<Badge variant={data.hosted ? "success" : "purple"}>{data.hosted ? "FREE" : "SELF-HOSTED"}</Badge>}
        />
        <div className="grid gap-4 sm:grid-cols-3 text-sm">
          <div><p className="text-xs text-muted">Plan</p><p className="mt-1 font-medium text-foreground capitalize">{data.organization.plan_code}</p></div>
          <div><p className="text-xs text-muted">Status</p><p className="mt-1 font-medium text-success capitalize">{data.organization.subscription_status}</p></div>
          <div><p className="text-xs text-muted">Current period (UTC)</p><p className="mt-1 font-medium text-foreground">{usage.period_start} → {usage.period_end}</p></div>
        </div>
      </Card>

      <Card>
        <CardHeader title="Usage & limits" subtitle={data.hosted ? `Free events reset on ${usage.period_end} UTC. Events are retained for ${limits.retention_days} days.` : "Subscription quotas are disabled for self-hosted deployments."} />
        <div className="grid gap-x-10 gap-y-5 sm:grid-cols-2">
          <LimitBar label="Accepted webhook events" used={usage.events_accepted} limit={limits.events} />
          <LimitBar label="Active custom domains" used={usage.domains} limit={limits.domains} />
          <LimitBar label="Providers" used={usage.providers} limit={limits.providers} />
          <LimitBar label="Active API keys" used={usage.api_keys} limit={limits.api_keys} />
          <LimitBar label="Users" used={usage.users} limit={limits.users} />
        </div>
      </Card>

      {canWrite ? <Card>
        <CardHeader
          title="Ingest domains"
          subtitle="Customer domains only receive webhooks. The dashboard remains on the trusin app domain."
          action={<Globe2 className="h-5 w-5 text-muted" />}
        />
        <div className="flex flex-col sm:flex-row gap-2 mb-5">
          <Input value={hostname} onChange={(event) => setHostname(event.target.value)} placeholder="hooks.example.com" onKeyDown={(event) => event.key === "Enter" && addDomain()} />
          <Button onClick={addDomain} loading={createDomain.isPending}>Add domain</Button>
        </div>
        {createDomain.isError && <p className="mb-4 text-sm text-danger">Could not add domain. Check the hostname and your plan limit.</p>}
        <div className="space-y-3">
          {domains.isLoading ? <p className="text-sm text-muted">Loading domains…</p> : domains.data?.length === 0 ? <p className="text-sm text-muted">No custom ingest domains yet.</p> : domains.data?.map((domain) => (
            <div key={domain.id} className="rounded-md border border-border bg-surface p-4">
              <div className="flex flex-wrap items-center gap-2 justify-between">
                <div className="flex items-center gap-2 min-w-0"><Globe2 className="h-4 w-4 shrink-0 text-success" /><code className="text-sm text-foreground truncate">{domain.hostname}</code><Badge variant={domain.status === "active" ? "success" : domain.status === "failed" ? "danger" : "warning"}>{domain.status}</Badge></div>
                <div className="flex gap-2"><Button variant="outline" size="sm" onClick={() => verifyDomain.mutate(domain.id)} loading={verifyDomain.isPending}>Verify</Button><Button variant="ghost" size="sm" aria-label={`Remove ${domain.hostname}`} onClick={() => setDomainPendingRemoval(domain.id)}><Trash2 className="h-4 w-4 text-danger" /></Button></div>
              </div>
              <div className="mt-3 grid gap-2 text-xs font-mono text-muted">
                <p>CNAME&nbsp; {domain.hostname} → <span className="text-foreground">{data.ingest_canonical_host}</span></p>
                <p>TXT&nbsp; _terusin-verification.{domain.hostname} → <span className="text-foreground">{domain.verification_token}</span></p>
              </div>
            </div>
          ))}
        </div>
      </Card> : <Card>
        <CardHeader title="Ingest domains" subtitle="Domain controls are available to workspace admins." action={<Globe2 className="h-5 w-5 text-muted" />} />
        <p className="text-sm text-secondary">You can view workspace usage, but only admins can add, verify, or remove custom ingest domains.</p>
      </Card>}

      <ConfirmDialog
        open={domainPendingRemoval !== null}
        onClose={() => setDomainPendingRemoval(null)}
        title="Remove ingest domain?"
        description="This domain will stop accepting webhooks through trusin. You can add it again later, but DNS changes may take time to propagate."
        confirmLabel="Remove domain"
        danger
        loading={deleteDomain.isPending}
        onConfirm={() => {
          if (!domainPendingRemoval) return;
          deleteDomain.mutate(domainPendingRemoval, { onSuccess: () => setDomainPendingRemoval(null) });
        }}
      />

      <Card>
        <CardHeader title="Organization API keys" subtitle="Keys belong to this organization and carry explicit scopes." action={<KeyRound className="h-5 w-5 text-muted" />} />
        <div className="flex flex-col sm:flex-row gap-3 sm:items-center sm:justify-between">
          <p className="text-sm text-secondary">{usage.api_keys} active key{usage.api_keys === 1 ? "" : "s"}. Create, inspect scopes, or revoke them from Settings.</p>
          <Button variant="outline" onClick={() => navigate("/settings")}><ExternalLink className="h-4 w-4" /> Manage keys</Button>
        </div>
      </Card>
    </div>
  );
}
