import { useState, type FormEvent } from "react";
import {
  Building2,
  CircleAlert,
  Globe2,
  KeyRound,
  Plus,
  Search,
  Users,
  Webhook,
} from "lucide-react";
import { Badge, Button, Card, CardHeader, Input } from "../components/ui";
import {
  usePlatformOrganization,
  usePlatformOrganizations,
  usePlatformOverview,
  useProvisionOrganization,
  useUpdatePlatformSubscription,
} from "../lib/hooks";
import { formatRelative } from "../lib/format";

const EMPTY_PROVISION = {
  name: "",
  slug: "",
  username: "",
  password: "",
  email: "",
  subscriber_name: "",
  billing_contact_name: "",
  billing_contact_email: "",
};

function Metric({ label, value, icon: Icon, tone = "text-success" }: { label: string; value: number; icon: typeof Building2; tone?: string }) {
  return (
    <Card className="p-4">
      <div className="flex items-center justify-between gap-3">
        <div><p className="text-2xl font-semibold text-foreground tabular-nums">{value.toLocaleString()}</p><p className="mt-1 text-xs text-muted">{label}</p></div>
        <div className={`rounded-md bg-surface p-2.5 ${tone}`}><Icon className="h-5 w-5" /></div>
      </div>
    </Card>
  );
}

function StatusBadge({ status }: { status: string }) {
  const variant = status === "active" ? "success" : status === "trialing" ? "warning" : "danger";
  return <Badge variant={variant}>{status}</Badge>;
}

export function Platform() {
  const [search, setSearch] = useState("");
  const [status, setStatus] = useState("");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [showProvision, setShowProvision] = useState(false);
  const [provision, setProvision] = useState(EMPTY_PROVISION);
  const overview = usePlatformOverview();
  const organizations = usePlatformOrganizations(search, status);
  const detail = usePlatformOrganization(selectedId);
  const createOrganization = useProvisionOrganization();
  const updateSubscription = useUpdatePlatformSubscription();
  const selected = detail.data?.organization;
  const [subscriptionDraft, setSubscriptionDraft] = useState<Record<string, string> | null>(null);

  const submitProvision = async (event: FormEvent) => {
    event.preventDefault();
    await createOrganization.mutateAsync({
      ...provision,
      email: provision.email || undefined,
      subscriber_name: provision.subscriber_name || undefined,
      billing_contact_name: provision.billing_contact_name || undefined,
      billing_contact_email: provision.billing_contact_email || undefined,
    });
    setProvision(EMPTY_PROVISION);
    setShowProvision(false);
  };

  const saveSubscription = async (event: FormEvent) => {
    event.preventDefault();
    if (!selected || !subscriptionDraft) return;
    await updateSubscription.mutateAsync({
      id: selected.id,
      subscriber_name: subscriptionDraft.subscriber_name,
      billing_contact_name: subscriptionDraft.billing_contact_name,
      billing_contact_email: subscriptionDraft.billing_contact_email,
      plan_code: subscriptionDraft.plan_code,
      subscription_status: subscriptionDraft.subscription_status,
    });
  };

  const updateDraft = (field: string, value: string) => {
    if (!selected) return;
    setSubscriptionDraft((draft) => ({
      ...(draft ?? {
        subscriber_name: selected.subscriber_name,
        billing_contact_name: selected.billing_contact_name,
        billing_contact_email: selected.billing_contact_email,
        plan_code: selected.plan_code,
        subscription_status: selected.subscription_status,
      }),
      [field]: value,
    }));
  };

  return (
    <div className="max-w-[1440px] space-y-6">
      <div className="flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
        <div><p className="text-xs font-semibold uppercase tracking-[.16em] text-success">Platform control plane</p><h1 className="mt-1 text-2xl font-semibold text-foreground">Tenants & subscriptions</h1><p className="mt-1 text-sm text-muted">Fleet-level operational data. Tenant event payloads are never shown here.</p></div>
        <Button onClick={() => setShowProvision((open) => !open)}><Plus className="h-4 w-4" /> Provision tenant</Button>
      </div>

      {overview.data && <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
        <Metric label="Active tenants" value={overview.data.active_organizations} icon={Building2} />
        <Metric label="Accepted events this period" value={overview.data.accepted_events} icon={Webhook} />
        <Metric label="Queued / retrying" value={overview.data.queued_events + overview.data.retrying_events} icon={CircleAlert} tone="text-warning" />
        <Metric label="Failed events (24h)" value={overview.data.failed_events_24h} icon={CircleAlert} tone="text-danger" />
      </div>}

      {showProvision && <Card>
        <CardHeader title="Provision a hosted tenant" subtitle="Creates one organization and its initial tenant admin. Subscription contact remains manually managed." />
        <form className="grid gap-3 md:grid-cols-2" onSubmit={submitProvision}>
          {(["name", "slug", "username", "password", "email", "subscriber_name", "billing_contact_name", "billing_contact_email"] as const).map((field) => (
            <label key={field} className="space-y-1.5 text-xs font-medium text-secondary">
              {field.replaceAll("_", " ")}
              <Input type={field === "password" ? "password" : "text"} required={["name", "slug", "username", "password"].includes(field)} value={provision[field]} onChange={(event) => setProvision((value) => ({ ...value, [field]: event.target.value }))} placeholder={field === "slug" ? "acme" : undefined} />
            </label>
          ))}
          <div className="md:col-span-2 flex gap-2 justify-end"><Button type="button" variant="ghost" onClick={() => setShowProvision(false)}>Cancel</Button><Button type="submit" loading={createOrganization.isPending}>Create tenant</Button></div>
          {createOrganization.isError && <p className="md:col-span-2 text-sm text-danger">Provisioning failed. Check fields and ensure the operator session is active.</p>}
        </form>
      </Card>}

      <div className="grid gap-6 xl:grid-cols-[minmax(0,1.25fr)_minmax(380px,.75fr)]">
        <Card className="min-w-0">
          <CardHeader title="Hosted tenants" subtitle={`${organizations.data?.total ?? 0} organizations`} />
          <div className="mb-4 flex gap-2">
            <div className="relative flex-1"><Search className="absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted" /><Input className="pl-9" value={search} onChange={(event) => setSearch(event.target.value)} placeholder="Search subscriber, tenant, or billing email" /></div>
            <select value={status} onChange={(event) => setStatus(event.target.value)} className="rounded-md border border-border bg-surface px-3 text-sm text-foreground"><option value="">All states</option><option value="active">Active</option><option value="trialing">Trialing</option><option value="cancelled">Cancelled</option></select>
          </div>
          <div className="overflow-x-auto"><table className="w-full text-left text-sm"><thead className="border-b border-border text-[10px] uppercase tracking-[.12em] text-muted"><tr><th className="pb-2 pr-3 font-medium">Subscriber</th><th className="pb-2 pr-3 font-medium">Plan</th><th className="pb-2 pr-3 font-medium">Usage</th><th className="pb-2 font-medium">Health</th></tr></thead><tbody>
            {organizations.data?.organizations.map((organization) => <tr key={organization.id} onClick={() => { setSelectedId(organization.id); setSubscriptionDraft(null); }} className={`cursor-pointer border-b border-border/70 transition-colors hover:bg-hover ${selectedId === organization.id ? "bg-[rgba(74,222,128,.06)]" : ""}`}><td className="py-3 pr-3"><p className="font-medium text-foreground">{organization.subscriber_name}</p><p className="text-xs text-muted">{organization.name} · {organization.billing_contact_email || "No billing email"}</p></td><td className="py-3 pr-3"><StatusBadge status={organization.subscription_status} /><p className="mt-1 text-xs text-muted capitalize">{organization.plan_code}</p></td><td className="py-3 pr-3 text-xs text-secondary">{organization.events_accepted.toLocaleString()} events<br />{organization.active_domains} domain · {organization.active_api_keys} keys</td><td className="py-3 text-xs text-secondary">{organization.queued_events + organization.retrying_events} in flight<br /><span className="text-muted">{organization.last_activity_at ? formatRelative(organization.last_activity_at) : "No activity"}</span></td></tr>)}
            {!organizations.isLoading && organizations.data?.organizations.length === 0 && <tr><td colSpan={4} className="py-10 text-center text-sm text-muted">No tenants match this filter.</td></tr>}
          </tbody></table></div>
        </Card>

        <Card className="min-w-0">
          {!selected && <div className="flex min-h-80 flex-col items-center justify-center text-center"><Building2 className="h-9 w-9 text-muted" /><p className="mt-3 text-sm font-medium text-foreground">Select a tenant</p><p className="mt-1 max-w-xs text-xs text-muted">Inspect subscription contact, aggregate health, domains, users, and API keys.</p></div>}
          {detail.isLoading && <p className="p-4 text-sm text-muted">Loading tenant detail…</p>}
          {selected && detail.data && <div className="space-y-5"><CardHeader title={selected.name} subtitle={`${selected.slug} · created ${formatRelative(selected.created_at)}`} action={<StatusBadge status={selected.subscription_status} />} />
            <div className="grid grid-cols-2 gap-3 text-xs"><div className="rounded-md border border-border bg-surface p-3"><p className="text-muted">24h delivered</p><p className="mt-1 text-lg font-semibold text-success">{detail.data.health_24h.delivered}</p></div><div className="rounded-md border border-border bg-surface p-3"><p className="text-muted">24h failed</p><p className="mt-1 text-lg font-semibold text-danger">{detail.data.health_24h.failed}</p></div></div>
            <form onSubmit={saveSubscription} className="space-y-3 border-t border-border pt-4"><p className="text-xs font-semibold uppercase tracking-[.12em] text-secondary">Subscription contact</p>{(["subscriber_name", "billing_contact_name", "billing_contact_email"] as const).map((field) => <label key={field} className="block text-xs text-muted">{field.replaceAll("_", " ")}<Input className="mt-1" value={subscriptionDraft?.[field] ?? selected[field]} onChange={(event) => updateDraft(field, event.target.value)} /></label>)}<div className="grid grid-cols-2 gap-2"><label className="text-xs text-muted">Plan<select value={subscriptionDraft?.plan_code ?? selected.plan_code} onChange={(event) => updateDraft("plan_code", event.target.value)} className="mt-1 w-full rounded-md border border-border bg-surface px-3 py-2 text-sm text-foreground"><option value="free">Free</option><option value="pro">Pro</option></select></label><label className="text-xs text-muted">Status<select value={subscriptionDraft?.subscription_status ?? selected.subscription_status} onChange={(event) => updateDraft("subscription_status", event.target.value)} className="mt-1 w-full rounded-md border border-border bg-surface px-3 py-2 text-sm text-foreground"><option value="active">Active</option><option value="trialing">Trialing</option><option value="cancelled">Cancelled</option></select></label></div><Button type="submit" size="sm" loading={updateSubscription.isPending}>Save subscription</Button></form>
            <div className="grid gap-3 border-t border-border pt-4 sm:grid-cols-3"><div><p className="flex items-center gap-1 text-xs text-muted"><Users className="h-3.5 w-3.5" /> Users</p><p className="mt-1 text-sm text-foreground">{detail.data.users.length}</p></div><div><p className="flex items-center gap-1 text-xs text-muted"><Globe2 className="h-3.5 w-3.5" /> Domains</p><p className="mt-1 text-sm text-foreground">{detail.data.domains.length}</p></div><div><p className="flex items-center gap-1 text-xs text-muted"><KeyRound className="h-3.5 w-3.5" /> API keys</p><p className="mt-1 text-sm text-foreground">{detail.data.api_keys.length}</p></div></div>
          </div>}
        </Card>
      </div>
    </div>
  );
}
