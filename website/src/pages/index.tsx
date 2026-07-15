import Link from '@docusaurus/Link';
import Layout from '@theme/Layout';
import Heading from '@theme/Heading';
import styles from './index.module.css';

const capabilities = [
  {number: '01', title: 'Delivery you can trust', text: 'Durable Postgres history, Redis-backed queues, exponential retry, and a complete attempt timeline.'},
  {number: '02', title: 'Operate with clarity', text: 'Inspect payloads, responses, latency, and failures from one focused operations dashboard.'},
  {number: '03', title: 'Route without friction', text: 'Map any provider to one or many targets with source-aware rules and secure HMAC signing.'},
  {number: '04', title: 'Built for your stack', text: 'Use the dashboard, CLI, HTTP API, or MCP server. Deploy as containers or compact Rust binaries.'},
];

const assurances = [
  ['At-least-once', 'Durable delivery'],
  ['SHA-256', 'Request signing'],
  ['RBAC', 'Admin & viewer roles'],
  ['Self-hosted', 'Your data, your infra'],
];

export default function Home(): JSX.Element {
  return (
    <Layout title="Enterprise webhook infrastructure" description="Reliable, observable, self-hosted webhook delivery infrastructure.">
      <main className={styles.page}>
        <section className={styles.hero}>
          <div className={styles.gridGlow} />
          <div className={`container ${styles.heroInner}`}>
            <div className={styles.heroCopy}>
              <div className={styles.eyebrow}><span /> WEBHOOK INFRASTRUCTURE, UNDER YOUR CONTROL</div>
              <Heading as="h1">Every webhook.<br /><em>Delivered with certainty.</em></Heading>
              <p className={styles.lead}>Terusin is the reliable delivery layer between your providers and services—built for teams that need control, visibility, and zero guesswork.</p>
              <div className={styles.actions}>
                <Link className={styles.primaryButton} to="/docs/intro">Start building <span>↗</span></Link>
                <Link className={styles.secondaryButton} to="/docs/concepts/architecture">Explore architecture <span>→</span></Link>
              </div>
              <div className={styles.heroMeta}>
                <span><i className={styles.liveDot} /> Open source</span>
                <span>Apache 2.0</span>
                <span>Built in Rust</span>
              </div>
            </div>

            <div className={styles.consoleWrap} aria-label="Terusin delivery console preview">
              <div className={styles.consoleGlow} />
              <div className={styles.console}>
                <div className={styles.consoleTop}>
                  <div className={styles.windowDots}><i /><i /><i /></div>
                  <span>LIVE DELIVERY</span>
                  <b><i /> OPERATIONAL</b>
                </div>
                <div className={styles.consoleBody}>
                  <div className={styles.endpoint}>
                    <span>INGEST ENDPOINT</span>
                    <code>POST /stripe/webhook</code>
                  </div>
                  <div className={styles.eventLine}>
                    <div className={styles.eventIcon}>↗</div>
                    <div><strong>payment_intent.succeeded</strong><small>stripe · evt_3Qz9...X81</small></div>
                    <time>12ms</time><b>DELIVERED</b>
                  </div>
                  <div className={styles.pipeline}>
                    <PipelineStep label="Received" value="0ms" active />
                    <div className={styles.connector} />
                    <PipelineStep label="Queued" value="2ms" active />
                    <div className={styles.connector} />
                    <PipelineStep label="Delivered" value="12ms" active />
                  </div>
                  <div className={styles.log}>
                    <span>12:48:06.421</span><code>target responded</code><b>200 OK</b>
                    <span>12:48:06.422</span><code>attempt recorded</code><b>✓</b>
                    <span>12:48:06.423</span><code>event completed</code><b>✓</b>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </section>

        <section className={styles.assuranceBar}>
          <div className="container">
            {assurances.map(([value, label]) => <div key={value}><strong>{value}</strong><span>{label}</span></div>)}
          </div>
        </section>

        <section className={styles.capabilities}>
          <div className="container">
            <div className={styles.sectionHeading}>
              <div><span className={styles.kicker}>ENGINEERED FOR RELIABILITY</span><Heading as="h2">Infrastructure that stays<br />out of your way.</Heading></div>
              <p>From the first request to the final response, every delivery is durable, observable, and fully under your control.</p>
            </div>
            <div className={styles.featureGrid}>
              {capabilities.map(item => <article className={styles.featureCard} key={item.number}>
                <span>{item.number}</span><div className={styles.featureMark}>{item.number === '01' ? '✓' : item.number === '02' ? '⌁' : item.number === '03' ? '⑂' : '⌘'}</div>
                <Heading as="h3">{item.title}</Heading><p>{item.text}</p><Link to="/docs/intro" aria-label={`Learn about ${item.title}`}>LEARN MORE <b>→</b></Link>
              </article>)}
            </div>
          </div>
        </section>

        <section className={styles.control}>
          <div className={`container ${styles.controlInner}`}>
            <div className={styles.controlCopy}>
              <span className={styles.kicker}>BUILT FOR SERIOUS OPERATIONS</span>
              <Heading as="h2">Your events.<br />Your infrastructure.<br /><em>Your rules.</em></Heading>
              <p>Keep sensitive payloads inside your environment while giving every team the tooling they need.</p>
              <ul>
                <li><i>✓</i><span><strong>Production-grade security</strong>JWT sessions, opaque API tokens, RBAC, and HMAC signatures.</span></li>
                <li><i>✓</i><span><strong>Deploy anywhere</strong>Run with Docker Compose, systemd, or compact standalone binaries.</span></li>
                <li><i>✓</i><span><strong>Automation first</strong>Manage delivery through the CLI, HTTP API, or an AI agent via MCP.</span></li>
              </ul>
            </div>
            <div className={styles.architectureCard}>
              <div className={styles.archHeader}><span>DELIVERY PIPELINE</span><b>TERUSIN CORE</b></div>
              <div className={styles.archFlow}>
                <ArchNode label="PROVIDERS" sub="Stripe · GitHub · Any" />
                <span>→</span><ArchNode label="TERUSIN" sub="Ingest · Queue · Route" primary />
                <span>→</span><ArchNode label="SERVICES" sub="Local · Cloud · Edge" />
              </div>
              <div className={styles.archFoundation}><span>POSTGRES<br /><small>Durable event history</small></span><span>REDIS<br /><small>High-speed delivery queue</small></span></div>
            </div>
          </div>
        </section>

        <section className={styles.cta}>
          <div className="container">
            <span className={styles.kicker}>SHIP WITH CONFIDENCE</span>
            <Heading as="h2">Stop hoping your webhooks arrive.</Heading>
            <p>Own the delivery layer your product depends on.</p>
            <div className={styles.actions}><Link className={styles.primaryButton} to="/docs/intro">Deploy Terusin <span>↗</span></Link><Link className={styles.secondaryButton} href="https://github.com/adityaputra11/terusin">View on GitHub <span>→</span></Link></div>
          </div>
        </section>
      </main>
    </Layout>
  );
}

function PipelineStep({label, value, active}: {label: string; value: string; active?: boolean}) {
  return <div className={styles.pipelineStep}><i className={active ? styles.active : ''}>✓</i><strong>{label}</strong><small>{value}</small></div>;
}

function ArchNode({label, sub, primary}: {label: string; sub: string; primary?: boolean}) {
  return <div className={`${styles.archNode} ${primary ? styles.archPrimary : ''}`}><i>{primary ? 'T' : '◆'}</i><strong>{label}</strong><small>{sub}</small></div>;
}
