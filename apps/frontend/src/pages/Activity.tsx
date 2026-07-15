import { useState } from "react";
import { ShieldCheck, UserRound, RefreshCw } from "lucide-react";
import { useAudit } from "../lib/hooks";
import { formatRelative } from "../lib/format";
import {
  Badge,
  Button,
  Card,
  CardHeader,
  EmptyState,
  FullSpinner,
  Pagination,
  Table,
  TBody,
  TD,
  TH,
  THead,
  TR,
} from "../components/ui";

function actionLabel(action: string) {
  return action.replaceAll(".", " / ").replaceAll("_", " ");
}

export function Activity() {
  const [page, setPage] = useState(1);
  const audit = useAudit(page, 25);
  const entries = audit.data?.entries ?? [];

  if (audit.isLoading) return <FullSpinner label="Loading activity..." />;

  return (
    <div className="space-y-6">
      <Card>
        <CardHeader
          title="Activity"
          subtitle="Security and operations audit trail"
          action={
            <Button variant="ghost" size="sm" onClick={() => audit.refetch()} loading={audit.isFetching}>
              <RefreshCw className="h-4 w-4" />
              Refresh
            </Button>
          }
        />

        {entries.length === 0 ? (
          <EmptyState
            icon={<ShieldCheck className="h-8 w-8" />}
            title="No activity recorded"
            description="Mutating actions and sign-ins appear here once the backend records them."
          />
        ) : (
          <>
            <Table>
              <THead>
                <TH>Action</TH>
                <TH>Actor</TH>
                <TH>Resource</TH>
                <TH>Metadata</TH>
                <TH className="text-right">Time</TH>
              </THead>
              <TBody>
                {entries.map((entry) => (
                  <TR key={entry.id}>
                    <TD>
                      <div className="flex items-center gap-2">
                        <ShieldCheck className="h-4 w-4 text-success" />
                        <span className="text-foreground font-medium capitalize">
                          {actionLabel(entry.action)}
                        </span>
                      </div>
                    </TD>
                    <TD>
                      <div className="flex items-center gap-2 text-secondary">
                        <UserRound className="h-4 w-4 text-muted" />
                        <span>{entry.actor_email ?? entry.actor_user_id?.slice(0, 8) ?? "system"}</span>
                      </div>
                    </TD>
                    <TD>
                      <div className="flex items-center gap-2">
                        <Badge variant="neutral">{entry.resource_type}</Badge>
                        {entry.resource_id && (
                          <code className="text-xs text-muted font-mono">{entry.resource_id.slice(0, 12)}</code>
                        )}
                      </div>
                    </TD>
                    <TD>
                      <code className="text-xs text-muted font-mono line-clamp-1">
                        {JSON.stringify(entry.metadata)}
                      </code>
                    </TD>
                    <TD className="text-right text-muted">
                      {formatRelative(entry.created_at)}
                    </TD>
                  </TR>
                ))}
              </TBody>
            </Table>
            <div className="pt-4">
              <Pagination
                page={audit.data?.page ?? page}
                pages={audit.data?.pages ?? 1}
                total={audit.data?.total ?? 0}
                onPageChange={setPage}
              />
            </div>
          </>
        )}
      </Card>
    </div>
  );
}
