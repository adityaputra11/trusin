import { useState } from "react";
import {
  Area,
  AreaChart,
  CartesianGrid,
  Cell,
  Pie,
  PieChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import { BarChart3, Activity, CheckCircle2, XCircle, Layers } from "lucide-react";
import { useMetrics, type MetricsRange } from "../lib/hooks";
import { Card, CardHeader, FullSpinner, Select } from "../components/ui";
import type { MetricsBucket } from "../types/api";

const RANGES: MetricsRange[] = ["24h", "7d", "30d"];

const PIE_COLORS = ["#22c55e", "#ef4444", "#f59e0b", "#3b82f6", "#9333ea", "#7a7a7a"];

function StatCard({
  icon,
  label,
  value,
  hint,
  accent = "text-foreground",
}: {
  icon: React.ReactNode;
  label: string;
  value: string | number;
  hint?: string;
  accent?: string;
}) {
  return (
    <Card className="p-4">
      <div className="flex items-center gap-2 text-muted mb-2">
        {icon}
        <span className="text-xs uppercase tracking-wide font-medium">{label}</span>
      </div>
      <p className={`text-2xl font-semibold ${accent}`}>{value}</p>
      {hint && <p className="text-xs text-muted mt-1">{hint}</p>}
    </Card>
  );
}

function ChartTooltip({ active, payload, label }: any) {
  if (!active || !payload?.length) return null;
  return (
    <div className="bg-card border border-border rounded-md px-3 py-2 text-xs shadow-card">
      <p className="text-muted mb-1">{label}</p>
      {payload.map((p: any, i: number) => (
        <p key={i} className="text-foreground font-medium">
          {p.name}: {p.value}
        </p>
      ))}
    </div>
  );
}

function formatBucket(iso: string, range: MetricsRange): string {
  const d = new Date(iso.replace(" ", "T"));
  if (range === "24h") {
    return d.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
  }
  return d.toLocaleDateString(undefined, { month: "short", day: "2-digit" });
}

export function Metrics() {
  const [range, setRange] = useState<MetricsRange>("24h");
  const { data, isLoading } = useMetrics(range);

  if (isLoading || !data) return <FullSpinner label="Loading metrics…" />;

  const series: { bucket: string; count: number }[] = data.series.map(
    (b: MetricsBucket) => ({
      bucket: formatBucket(b.bucket, range),
      count: b.count,
    }),
  );

  const statusPie = [
    { name: "Delivered", value: data.delivered },
    { name: "Failed", value: data.failed },
    { name: "Other", value: Math.max(0, data.total - data.delivered - data.failed) },
  ].filter((d) => d.value > 0);

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <BarChart3 className="h-5 w-5 text-muted" />
          <h2 className="text-base font-semibold text-foreground">Metrics</h2>
        </div>
        <Select
          value={range}
          onChange={(e) => setRange(e.target.value as MetricsRange)}
          className="w-32"
        >
          {RANGES.map((r) => (
            <option key={r} value={r}>
              {r}
            </option>
          ))}
        </Select>
      </div>

      <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
        <StatCard
          icon={<Activity className="h-4 w-4" />}
          label="Total events"
          value={data.total}
          hint={`last ${data.range_hours}h`}
        />
        <StatCard
          icon={<CheckCircle2 className="h-4 w-4 text-success" />}
          label="Success rate"
          value={`${data.success_rate.toFixed(1)}%`}
          hint={`${data.delivered} delivered`}
          accent="text-success"
        />
        <StatCard
          icon={<XCircle className="h-4 w-4 text-danger" />}
          label="Failed"
          value={data.failed}
          accent="text-danger"
        />
        <StatCard
          icon={<Layers className="h-4 w-4 text-warning" />}
          label="In queue"
          value={data.queue_depth + data.retry_depth}
          hint={`${data.queue_depth} queued · ${data.retry_depth} retrying`}
          accent="text-warning"
        />
      </div>

      <Card>
        <CardHeader
          title="Throughput"
          subtitle={`Events per ${range === "24h" ? "hour" : "day"}`}
        />
        <div className="h-64 w-full">
          <ResponsiveContainer width="100%" height="100%">
            <AreaChart data={series} margin={{ top: 10, right: 16, left: 0, bottom: 0 }}>
              <defs>
                <linearGradient id="throughputFill" x1="0" y1="0" x2="0" y2="1">
                  <stop offset="5%" stopColor="#22c55e" stopOpacity={0.4} />
                  <stop offset="95%" stopColor="#22c55e" stopOpacity={0} />
                </linearGradient>
              </defs>
              <CartesianGrid strokeDasharray="3 3" stroke="#1a1a1a" vertical={false} />
              <XAxis
                dataKey="bucket"
                stroke="#7a7a7a"
                tick={{ fontSize: 11 }}
                tickLine={false}
                axisLine={{ stroke: "#232323" }}
                minTickGap={24}
              />
              <YAxis
                stroke="#7a7a7a"
                tick={{ fontSize: 11 }}
                tickLine={false}
                axisLine={false}
                allowDecimals={false}
                width={32}
              />
              <Tooltip content={<ChartTooltip />} />
              <Area
                type="monotone"
                dataKey="count"
                stroke="#22c55e"
                strokeWidth={2}
                fill="url(#throughputFill)"
              />
            </AreaChart>
          </ResponsiveContainer>
        </div>
      </Card>

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        <Card>
          <CardHeader title="Status breakdown" />
          <div className="h-56 w-full">
            {statusPie.length === 0 ? (
              <p className="text-sm text-muted text-center pt-20">No data</p>
            ) : (
              <ResponsiveContainer width="100%" height="100%">
                <PieChart>
                  <Pie
                    data={statusPie}
                    dataKey="value"
                    nameKey="name"
                    cx="50%"
                    cy="50%"
                    innerRadius={50}
                    outerRadius={80}
                    paddingAngle={2}
                  >
                    {statusPie.map((_, i) => (
                      <Cell key={i} fill={PIE_COLORS[i % PIE_COLORS.length]} />
                    ))}
                  </Pie>
                  <Tooltip content={<ChartTooltip />} />
                </PieChart>
              </ResponsiveContainer>
            )}
          </div>
          <div className="flex flex-wrap gap-3 px-4 pb-4">
            {statusPie.map((d, i) => (
              <div key={d.name} className="flex items-center gap-1.5 text-xs">
                <span
                  className="h-2 w-2 rounded-full"
                  style={{ background: PIE_COLORS[i % PIE_COLORS.length] }}
                />
                <span className="text-secondary">{d.name}</span>
                <span className="text-muted">{d.value}</span>
              </div>
            ))}
          </div>
        </Card>

        <Card className="p-0">
          <div className="p-4 border-b border-border">
            <h3 className="text-sm font-semibold text-foreground">Top sources</h3>
          </div>
          {data.top_sources.length === 0 ? (
            <p className="text-sm text-muted p-4">No data</p>
          ) : (
            <ul className="divide-y divide-border">
              {data.top_sources.map((s, i) => (
                <li key={i} className="flex items-center justify-between px-4 py-2.5 text-sm">
                  <span className="font-mono text-secondary truncate mr-3">
                    {s.source || "—"}
                  </span>
                  <span className="text-muted tabular-nums">{s.count}</span>
                </li>
              ))}
            </ul>
          )}
          <div className="p-4 border-t border-border mt-1">
            <h3 className="text-sm font-semibold text-foreground mb-2">Top targets</h3>
            {data.top_targets.length === 0 ? (
              <p className="text-sm text-muted">No data</p>
            ) : (
              <ul className="space-y-2">
                {data.top_targets.map((t, i) => (
                  <li
                    key={i}
                    className="flex items-center justify-between gap-3 text-xs"
                  >
                    <code className="font-mono text-muted truncate">
                      {t.target || "—"}
                    </code>
                    <span className="text-muted tabular-nums shrink-0">{t.count}</span>
                  </li>
                ))}
              </ul>
            )}
          </div>
        </Card>
      </div>
    </div>
  );
}
