import { invokeTyped } from "@/lib/userActions";

export function sshHostsList<T>() {
  return invokeTyped<T>("get_ssh_hosts");
}

export function sshTunnelsSnapshot<T>() {
  return invokeTyped<T>("ssh_tunnels_snapshot");
}

export function sshTunnelsRefreshStatus<T>() {
  return invokeTyped<T>("ssh_tunnels_refresh_status");
}

export function sshTunnelGroupUpsert<T>(input: {
  id?: string;
  name: string;
}) {
  return invokeTyped<T>("ssh_tunnel_group_upsert", { input });
}

export function sshTunnelGroupDelete<T>(id: string) {
  return invokeTyped<T>("ssh_tunnel_group_delete", { id });
}

export function sshTunnelUpsert<T>(input: Record<string, unknown>) {
  return invokeTyped<T>("ssh_tunnel_upsert", { input });
}

export function sshTunnelProbeDraft<T>(input: Record<string, unknown>) {
  return invokeTyped<T>("ssh_tunnel_probe_draft", { input });
}

export function sshTunnelProbeSaved<T>(id: string) {
  return invokeTyped<T>("ssh_tunnel_probe_saved", { id });
}

export function sshTunnelConnect<T>(id: string) {
  return invokeTyped<T>("ssh_tunnel_connect", { id });
}

export function sshTunnelDisconnect<T>(id: string) {
  return invokeTyped<T>("ssh_tunnel_disconnect", { id });
}

export function sshTunnelDelete<T>(id: string) {
  return invokeTyped<T>("ssh_tunnel_delete", { id });
}

export function sshTunnelGroupConnect<T>(groupId: string) {
  return invokeTyped<T>("ssh_tunnel_group_connect", { groupId });
}

export function sshTunnelGroupDisconnect<T>(groupId: string) {
  return invokeTyped<T>("ssh_tunnel_group_disconnect", { groupId });
}
