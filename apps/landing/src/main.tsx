import { StrictMode, useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import "./styles.css";

const config = {
  appUrl: import.meta.env.VITE_APP_URL || "https://app.terusin-dev.my.id",
  docsUrl: import.meta.env.VITE_DOCS_URL || "https://docs.terusin-dev.my.id",
  githubUrl:
    import.meta.env.VITE_GITHUB_URL || "https://github.com/adityaputra11/terusin",
  managedEmail: import.meta.env.VITE_MANAGED_CONTACT_EMAIL || "",
};

const managedCta = config.managedEmail
  ? {
      href: `mailto:${config.managedEmail}?subject=${encodeURIComponent("Managed trusin instance")}`,
      label: "Get a managed instance",
    }
  : { href: config.appUrl, label: "Open hosted app" };

function Arrow() {
  return <span aria-hidden="true">↗</span>;
}

function Logo() {
  return (
    <a className="logo" href="#top" aria-label="trusin home">
      <img className="logo-mark" src="/icon-trusin.png" alt="" />
      <span>trusin</span>
      <i aria-hidden="true" />
    </a>
  );
}

function GitHubStars() {
  const [stars, setStars] = useState<number | null>(null);

  useEffect(() => {
    let active = true;
    fetch("https://api.github.com/repos/adityaputra11/terusin", {
      headers: { Accept: "application/vnd.github+json" },
    })
      .then((response) => (response.ok ? response.json() : null))
      .then((repository: { stargazers_count?: number } | null) => {
        if (active && repository?.stargazers_count !== undefined) {
          setStars(repository.stargazers_count);
        }
      })
      .catch(() => undefined);
    return () => {
      active = false;
    };
  }, []);

  if (stars === null) return null;
  return (
    <a
      className="github-nav-link"
      href={config.githubUrl}
      target="_blank"
      rel="noreferrer"
      aria-label={`trusin on GitHub, ${stars} stars`}
    >
      <span>GitHub</span>
      <b aria-hidden="true">★</b> {stars.toLocaleString()}
    </a>
  );
}

function ConsolePreview() {
  return (
    <div className="console-shell" aria-label="Webhook delivery console preview">
      <div className="console-glow" />
      <div className="console">
        <div className="console-bar">
          <span className="dots"><i /><i /><i /></span>
          <span>DELIVERY TRACE</span>
          <b><i /> LIVE</b>
        </div>
        <div className="console-content">
          <div className="console-endpoint"><span>INGEST ENDPOINT</span><code>POST /stripe/webhook</code></div>
          <div className="trace-head">
            <span className="trace-icon">↗</span>
            <div><strong>payment_intent.succeeded</strong><small>stripe · evt_3Qz9…X81</small></div>
            <em>12ms</em><b>DELIVERED</b>
          </div>
          <div className="trace-flow">
            <TraceStep label="Received" value="0ms" />
            <span className="trace-line" />
            <TraceStep label="Queued" value="2ms" />
            <span className="trace-line" />
            <TraceStep label="Delivered" value="12ms" />
          </div>
          <div className="console-log">
            <span>12:48:06.421</span><code>target responded</code><b>200 OK</b>
            <span>12:48:06.422</span><code>attempt recorded</code><b>✓</b>
            <span>12:48:06.423</span><code>event completed</code><b>✓</b>
          </div>
        </div>
      </div>
    </div>
  );
}

function TraceStep({ label, value }: { label: string; value: string }) {
  return <div className="trace-step"><i>✓</i><strong>{label}</strong><small>{value}</small></div>;
}

function App() {
  return (
    <div id="top" className="page">
      <header className="nav-wrap">
        <nav className="nav container" aria-label="Main navigation">
          <Logo />
          <div className="nav-links">
            <a href="#product">Product</a>
            <a href="#hosting">Managed hosting</a>
            <a href={config.docsUrl}>Docs</a>
            <GitHubStars />
          </div>
          <a className="nav-cta" href={config.appUrl}>Open app <Arrow /></a>
        </nav>
      </header>

      <main>
        <section className="hero">
          <div className="grid" aria-hidden="true" />
          <div className="container hero-content">
            <div className="hero-copy">
              <p className="eyebrow"><i /> WEBHOOK INFRASTRUCTURE, UNDER YOUR CONTROL</p>
              <h1>Every webhook.<br /><em>Delivered with certainty.</em></h1>
              <p className="hero-lead">trusin is the reliable delivery layer between your providers and services—built for teams that need control, visibility, and zero guesswork.</p>
              <div className="hero-actions">
                <a className="button button-primary" href={managedCta.href}>{managedCta.label} <Arrow /></a>
                <a className="button button-secondary" href={`${config.docsUrl}/docs/intro`}>Self-host trusin <span aria-hidden="true">→</span></a>
              </div>
              <div className="hero-proof"><span><i /> Open source</span><span>Apache 2.0</span><span>Built in Rust</span></div>
            </div>
            <ConsolePreview />
          </div>
        </section>

        <section className="stats" aria-label="trusin guarantees">
          <div className="container stats-grid">
            <Stat value="At-least-once" label="Durable delivery" />
            <Stat value="SHA-256" label="Request signing" />
            <Stat value="RBAC" label="Admin & viewer roles" />
            <Stat value="Self-hosted" label="Your data, your infra" />
          </div>
        </section>

        <section id="product" className="section features container">
          <div className="section-heading">
            <div><p className="kicker">ENGINEERED FOR RELIABILITY</p><h2>Infrastructure that stays<br />out of your way.</h2></div>
            <p>From the first request to the final response, every delivery is durable, observable, and fully under your control.</p>
          </div>
          <div className="feature-grid">
            <Feature number="01" glyph="✓" title="Delivery you can trust" text="Postgres history, Redis-backed queues, exponential retry, and a complete delivery timeline." />
            <Feature number="02" glyph="⌁" title="Operate with clarity" text="Inspect payloads, responses, latency, and failures from one focused operations dashboard." />
            <Feature number="03" glyph="⑂" title="Route without friction" text="Map any provider to one or many targets with source-aware rules and secure HMAC signing." />
            <Feature number="04" glyph="⌘" title="Built for your stack" text="Use the dashboard, CLI, HTTP API, or MCP server. Run it as containers or Rust binaries." />
          </div>
        </section>

        <section className="section workflow">
          <div className="container workflow-grid">
            <div className="workflow-copy"><p className="kicker">ONE CLEAR DELIVERY PATH</p><h2>Receive it. Inspect it.<br /><em>Keep it moving.</em></h2><p>Every event gets a durable history, an observable delivery attempt, and a safe recovery path when a downstream service fails.</p><a className="text-link" href={`${config.docsUrl}/docs/concepts/reliability`}>Explore reliability <span>→</span></a></div>
            <div className="pipeline-card">
              <div className="pipeline-bar"><span>DELIVERY PIPELINE</span><b>TRUSIN CORE</b></div>
              <div className="pipeline-nodes"><Node label="PROVIDERS" sub="Stripe · GitHub · Any" icon="◆" /><span>→</span><Node label="TRUSIN" sub="Ingest · Queue · Route" icon="T" primary /><span>→</span><Node label="SERVICES" sub="Local · Cloud · Edge" icon="◆" /></div>
              <div className="pipeline-foundation"><span>POSTGRES<small>Durable event history</small></span><span>REDIS<small>High-speed delivery queue</small></span></div>
            </div>
          </div>
        </section>

        <section className="section api-client container">
          <div className="api-client-copy"><p className="kicker">COMING NEXT</p><h2>Your webhook control room is getting an API workspace.</h2><p>trusin is expanding into a focused REST API client for building, running, saving, and sharing HTTP requests alongside the webhook events they trigger.</p><ul><li><i>✓</i> Request builder, response viewer, and request history</li><li><i>✓</i> Environments, variables, and redacted secret handling</li><li><i>✓</i> Collections plus OpenAPI, cURL, and Postman import</li></ul></div>
          <div className="request-preview" aria-label="API client concept preview"><div className="request-top"><span>API CLIENT</span><b>IN DEVELOPMENT</b></div><div className="request-url"><strong>POST</strong><code>https://api.acme.test/v1/orders</code><button type="button" tabIndex={-1}>Send</button></div><div className="request-body"><div><span>Headers</span><span>Body</span><span>Auth</span></div><code>{'{'}<br />&nbsp;&nbsp;"amount": 12000,<br />&nbsp;&nbsp;"currency": "IDR"<br />{'}'}</code></div><div className="response"><span>201 Created <i>184ms</i></span><code>{'{'} "id": "ord_87q9", "status": "created" {'}'}</code></div></div>
        </section>

        <section id="hosting" className="section hosting">
          <div className="container hosting-inner"><div><p className="kicker">MANAGED TRUSIN</p><h2>Keep the control.<br /><em>Skip the operations.</em></h2><p>Let us run an isolated trusin instance for your team. Your webhook data, database, and delivery pipeline stay separate—without the maintenance overhead.</p></div><div className="hosting-points"><Point title="Isolated by design" text="A dedicated instance and database for your team. No shared event stream." /><Point title="Production ready" text="Managed upgrades, backups, monitoring, and help when delivery matters." /><Point title="Still yours" text="Move to self-hosted any time. trusin remains open source under Apache 2.0." /></div></div>
        </section>

        <section className="cta"><div className="container"><p className="kicker">SHIP WITH CONFIDENCE</p><h2>Stop hoping your webhooks arrive.</h2><p>Own the delivery layer your product depends on—self-host it, or let us run it for you.</p><div className="hero-actions"><a className="button button-primary" href={managedCta.href}>{managedCta.label} <Arrow /></a><a className="button button-secondary" href={config.githubUrl}>View on GitHub <span>→</span></a></div></div></section>
      </main>

      <footer className="footer"><div className="container"><Logo /><span>© {new Date().getFullYear()} trusin. Apache-2.0.</span><div><a href={config.docsUrl}>Documentation</a><a href={config.githubUrl}>GitHub</a><a href={config.appUrl}>Open app</a></div></div></footer>
    </div>
  );
}

function Stat({ value, label }: { value: string; label: string }) { return <div><strong>{value}</strong><span>{label}</span></div>; }
function Feature({ number, glyph, title, text }: { number: string; glyph: string; title: string; text: string }) { return <article className="feature"><span>{number}</span><i>{glyph}</i><h3>{title}</h3><p>{text}</p></article>; }
function Node({ label, sub, icon, primary = false }: { label: string; sub: string; icon: string; primary?: boolean }) { return <div className={`pipeline-node ${primary ? "primary" : ""}`}><i>{icon}</i><strong>{label}</strong><small>{sub}</small></div>; }
function Point({ title, text }: { title: string; text: string }) { return <article><i>✓</i><div><h3>{title}</h3><p>{text}</p></div></article>; }

createRoot(document.getElementById("root")!).render(<StrictMode><App /></StrictMode>);
