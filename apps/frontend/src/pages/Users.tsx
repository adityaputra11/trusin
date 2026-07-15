import { FormEvent, useState } from "react";
import { Mail, Shield, UserRound } from "lucide-react";
import { useCreateInvite, useInvites, useOrganization, useResendInvite, useRevokeInvite, useUpdateUserRole, useUsers } from "../lib/hooks";
import { formatRelative } from "../lib/format";
import { useCurrentUser } from "../lib/user-context";
import {
  Badge,
  Button,
  Card,
  CardHeader,
  EmptyState,
  FullSpinner,
  Input,
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
  const organization = useOrganization();
  const invites = useInvites();
  const createInvite = useCreateInvite();
  const resendInvite = useResendInvite();
  const revokeInvite = useRevokeInvite();
  const [email, setEmail] = useState("");
  const [role, setRole] = useState<"admin" | "viewer">("viewer");
  const isFree = organization.data?.organization.plan_code === "free";

  const submitInvite = (event: FormEvent) => {
    event.preventDefault();
    if (!email.trim() || isFree) return;
    createInvite.mutate({ email: email.trim(), role }, { onSuccess: () => setEmail("") });
  };

  if (users.isLoading) return <FullSpinner label="Loading users..." />;

  return (
    <Card>
      <CardHeader
        title="Users"
        subtitle="Single-workspace access and roles"
        action={<Badge variant="success">admin only</Badge>}
      />

      <div className="border-b border-border p-5">
        <div className="mb-3 flex items-center justify-between gap-3">
          <div>
            <p className="text-sm font-medium text-foreground">Invite a user</p>
            <p className="text-xs text-muted">Invited users sign in with the exact Google email you enter.</p>
          </div>
          {isFree && <Badge variant="neutral">Upgrade required</Badge>}
        </div>
        {isFree ? (
          <p className="rounded-md border border-border bg-hover px-3 py-2 text-sm text-muted">
            Free workspaces are limited to the owner. Upgrade to invite collaborators.
          </p>
        ) : (
          <form onSubmit={submitInvite} className="flex flex-col gap-2 sm:flex-row">
            <Input type="email" value={email} onChange={(event) => setEmail(event.target.value)} placeholder="teammate@example.com" required />
            <Select value={role} onChange={(event) => setRole(event.target.value as "admin" | "viewer")} className="sm:w-32">
              <option value="viewer">viewer</option>
              <option value="admin">admin</option>
            </Select>
            <Button type="submit" loading={createInvite.isPending}><Mail className="h-4 w-4" />Invite</Button>
          </form>
        )}
      </div>

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

      {invites.data && invites.data.length > 0 && (
        <div className="border-t border-border p-5">
          <p className="mb-3 text-sm font-medium text-foreground">Invitations</p>
          <Table>
            <THead><TH>Email</TH><TH>Role</TH><TH>Status</TH><TH className="text-right">Actions</TH></THead>
            <TBody>
              {invites.data.map((invite) => {
                const status = invite.accepted_at ? "accepted" : invite.revoked_at ? "revoked" : new Date(invite.expires_at) < new Date() ? "expired" : "pending";
                return <TR key={invite.id}>
                  <TD>{invite.email}</TD>
                  <TD><Badge variant="neutral">{invite.role}</Badge></TD>
                  <TD><Badge variant={status === "pending" ? "success" : "neutral"}>{status}</Badge></TD>
                  <TD className="text-right">
                    {status === "pending" && <div className="flex justify-end gap-2">
                      <Button size="sm" variant="outline" loading={resendInvite.isPending} onClick={() => resendInvite.mutate(invite.id)}>Resend</Button>
                      <Button size="sm" variant="outline" loading={revokeInvite.isPending} onClick={() => revokeInvite.mutate(invite.id)}>Revoke</Button>
                    </div>}
                  </TD>
                </TR>;
              })}
            </TBody>
          </Table>
        </div>
      )}
    </Card>
  );
}
