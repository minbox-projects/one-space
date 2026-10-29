# Network Proxy

## Overview

OneSpace has one global network proxy source. **Settings → Services → Network Proxy** is the only editor for application-managed HTTP(S) routing; the AI Gateway has no independent upstream-proxy field or override. A successful save changes the transport used by the next admitted request or retry without restarting OneSpace. Requests that were already admitted may finish on their existing transport.

The runtime publishes an immutable, validated proxy snapshot. The snapshot carries a generation that changes only when the effective identity changes: enabled state, proxy type, host, port, username or the decrypted password. An equivalent save, or a save that only changes the health-check interval, keeps the generation and does not restart cached transport or the OpenCode Zen relay.

## Coverage

The global proxy applies to application-owned HTTP(S) traffic:

| Area | Covered operations |
| --- | --- |
| Proxy diagnostics | Saved-proxy health check and isolated unsaved-draft connection test |
| Mail and Google OAuth | Gmail HTTP bridge, token exchange and refresh |
| AI News and CLI updates | RSS sources, release metadata, npm metadata and fallback requests |
| AI Environments | Provider model-list requests |
| AI Gateway | Ordinary forwarding and retries, CommandCode quota, OpenCode Go usage, template/model/catalog synchronization and Zen relay egress |
| Protocol Router | HTTP upstream forwarding |
| MCP | npm package-version metadata |
| HTTP tools | Short links and HTTP/cURL playground requests |
| Webhook catcher | Replay and mirror forwarding |
| Uptime monitor | HTTP probes and alert webhooks |
| SSL certificate tool | DoH, Cloudflare API and all ACME HTTP operations |
| Application updates | Native update check/download, GitHub fallback and current-version release notes |
| Source synchronization | Newly spawned app-owned Git HTTPS tasks, including Skills and Subagents source paths |

Each new operation and each retry resolves the current snapshot. Long sequential, fallback and polling batches no longer reuse a client captured at the start of the batch. App-owned short-lived Git child processes receive a consistent current proxy environment at command admission; a process that is already running is not changed.

## Boundaries

The proxy does not apply to:

- SSH or SFTP connections.
- Native TCP, Ping or raw DNS probes.
- Direct TLS certificate inspection that does not use an HTTP client.
- Local listeners or application IPC.
- External browsers, terminals, user-owned CLI processes or an already running Git process.
- MCP HTTP/SSE connection probes, which external MCP CLI processes perform; only MCP npm package-version metadata requests are application-owned and covered above.
- User Git configuration; OneSpace passes environment variables to the child it starts and does not rewrite user configuration.

Loopback destinations `localhost`, `127.0.0.1` and `::1` always connect directly in every proxy state. There is no general private-network bypass.

## Direct mode, loopback and failure handling

When the proxy is disabled or saved as `None`, managed HTTP transport is explicitly direct. It does not inherit an operating-system proxy, an environment proxy or a previously configured proxy.

Saving validates and prepares the transport and persists the device configuration before publishing the runtime snapshot. A validation error, a client-preparation failure or a persistence failure leaves the saved and active proxy unchanged. A saved configuration that is valid but unreachable fails through that proxy; it does not silently fall back to the previous proxy or to direct access.

Testing an unsaved draft uses that draft for the test only. Reloading a settings draft does not publish a change.

## AI Gateway

AI Gateway forwarding, quota, usage, template/catalog fetches and the Zen relay all use the global proxy snapshot. Clients are cached by proxy generation, and single-flight work is separated by generation: a refresh admitted after a switch starts independently, while matching requests within one generation still share one flight. A late result from an older generation settles its own callers but cannot replace the newer generation's cache, template state or failure state. The local gateway-to-relay client remains loopback-only and direct.

Gateway configuration uses schema 6. On the first read of a schema-5 file, the version-gated migration removes only the obsolete `settings.proxy_url`; it preserves all other settings, providers, keys, mappings, prices and pools, and does not copy the old URL into global settings. A schema-5 file skips earlier migration gates. Repeated reads of the current schema do not migrate or rewrite, and a future schema number is not downgraded.

Rollback limitation: the removed independent gateway URL cannot be reconstructed from current data. There is no automatic backup for this step. Recovering it requires a pre-change encrypted gateway backup that still contains `settings.proxy_url`, or manually re-entering the value in the global network proxy editor. Reverting the application does not restore the field by rewriting current data.

## OpenCode Zen relay

The Zen relay is a separate BoringSSL process behind a loopback endpoint. Its identity is the global proxy generation, the explicit proxy URL derived from the snapshot and the configured read timeout. A new generation starts a new relay child while admitted ordinary and SSE responses keep a lease on the retiring child; the child exits after its last response body finishes. An equivalent save or a check-interval-only save reuses the current child. Gateway or application shutdown terminates every owned current and retiring child. A missing or unusable transport fails the attempt explicitly and never falls back to direct Zen traffic.

## ACME and application updates

ACME account construction injects a managed HTTP adapter at both account-builder sites. Every SDK HTTP operation, including directory, account, order, authorization, finalization, polling and certificate download, resolves the current proxy transport instead of capturing it when the account was created. Account and order identity are preserved across a switch.

Application updates use the official `tauri-plugin-updater` 2.9.0. The Rust `configure_client` hook resolves the current snapshot each time the SDK builds a check or download client. The application owns typed `Update` handles and the bytes returned after official signature verification, and exposes `app_updater_check`, `app_updater_download`, `app_updater_install` and `app_updater_discard`; shutdown clears the retained entries. A proxy change after a check makes the next download use the new transport, while already verified bytes remain available for installation. The frontend facade keeps its existing metadata, state, progress, error and result behavior, and routes GitHub fallback and release-note requests through the managed Rust HTTP bridge. The Rust plugin registration, updater endpoint and public key remain; the JavaScript updater package and `updater:default` permission are removed.

## Verification status

Behavior is covered with local proxy, upstream, TLS and process fixtures plus mocks. The following checks were **not executed** and are not claimed as passing:

- Native macOS or Windows GUI, ACL and operating-system proxy integration.
- Real AI provider traffic.
- Real certificate authority, public DNS or Cloudflare service interaction.
- Real update installation.
