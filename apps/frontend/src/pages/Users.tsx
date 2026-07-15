import { Shield, UserRound } from "lucide-react";
import { useUpdateUserRole, useUsers } from "../lib/hooks";
import { formatRelative } from "../lib/format";
import { useCurrentUser } from "../lib/user-context";
import {
  Badge,
  Card,
  CardHeader,
  EmptyState,
  FullSpinner,
  Select,
  Table,
  TBody,
  TD,
  TH,
  THead,
  TR,
} from "../components/ui";

export function Users() {
  const users = useUsers();
  const updateRole = useUpdateUserRole();
  const currentUser = useCurrentUser();

  if (users.isLoading) return <FullSpinner label="Loading users..." />;

  return (
    <Card>
      <CardHeader
        title="Users"
        subtitle="Single-workspace access and roles"
        action={<Badge variant="success">admin only</Badge>}
      />

      {!users.data || users.data.length === 0 ? (
        <EmptyState
          icon={<UserRound className="h-8 w-8" />}
          title="No users yet"
          description="Google and password users appear here after first sign-in."
        />
      ) : (
        <Table>
          <THead>
            <TH>User</TH>
            <TH>Auth</TH>
            <TH>Role</TH>
            <TH className="text-right">Created</TH>
          </THead>
          <TBody>
            {users.data.map((user) => (
              <TR key={user.id}>
                <TD>
                  <div className="flex items-center gap-3">
                    {user.avatar_url ? (
                      <img
                        src={user.avatar_url}
                        alt=""
                        className="h-8 w-8 rounded-md object-cover border border-border-light"
                        referrerPolicy="no-referrer"
                      />
                    ) : (
                      <div className="h-8 w-8 rounded-md bg-hover border border-border-light grid place-items-center">
                        <UserRound className="h-4 w-4 text-muted" />
                      </div>
                    )}
                    <div>
                      <p className="text-sm text-foreground font-medium">
                        {user.display_name ?? user.username ?? user.email ?? "Unnamed user"}
                      </p>
                      <p className="text-xs text-muted">{user.email ?? user.username ?? user.id}</p>
                    </div>
                  </div>
                </TD>
                <TD>
                  <Badge variant={user.oauth_provider ? "success" : "neutral"}>
                    {user.oauth_provider ?? "password"}
                  </Badge>
                </TD>
                <TD>
                  <div className="flex items-center gap-2">
                    <Shield className="h-4 w-4 text-muted" />
                    <Select
                      value={user.role}
                      disabled={updateRole.isPending}
                      onChange={(event) =>
                        updateRole.mutate({
                          id: user.id,
                          role: event.target.value as "admin" | "viewer",
                        })
                      }
                      title={currentUser?.id === user.id ? "Your role" : "Update role"}
                    >
                      <option value="admin">admin</option>
                      <option value="viewer">viewer</option>
                    </Select>
                  </div>
                </TD>
                <TD className="text-right text-muted">{formatRelative(user.created_at)}</TD>
              </TR>
            ))}
          </TBody>
        </Table>
      )}
    </Card>
  );
}
