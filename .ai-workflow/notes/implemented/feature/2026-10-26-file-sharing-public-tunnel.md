# Agent Note: File Sharing Public Access via an Ephemeral Cloudflare Tunnel

Status: implemented

English | [中文](2026-10-26-file-sharing-public-tunnel.zh.md)

## Problem

File Sharing originally served selected files only through a private IPv4 HTTP link. Users needed to hand a temporary download link to a recipient on another network without configuring a saved port tunnel first. Port Tunnel already owns persisted Cloudflare and FRP definitions bound to fixed local ports, while a file share has a randomly assigned port, a per-session token and a lifetime owned by the share itself. Reusing a saved tunnel would couple two independent lifecycles, require users to create or edit a saved entry for each share, and let ephemeral shares appear in a saved configuration list. The feature also needed an optional password because a public link is reachable by anyone who has it.

## Decision

File Sharing keeps the stable `file-sharing` toolbox id and remains LAN-only by default. The start input gains optional `publicAccess` and `password`; the snapshot gains `passwordProtected`, `publicTunnelId`, `publicState` (`disabled`, `starting`, `running`, `error`), `publicShareUrl` and `publicError`; `file_sharing_public_start` and `file_sharing_public_stop` toggle public access on a running share; `file-sharing-public-updated` carries share-owned tunnel status and is filtered by the current tunnel id.

Public access is a share-owned ephemeral Cloudflare Quick Tunnel. The runtime targets the address and port of its own bound listener, names the tunnel with the reserved `file-sharing-` prefix, and uses a fresh unique id for every attempt. The tunnel is tracked in the same process supervisor as saved tunnels but is never written to `tunnels.json`, never appears in saved-tunnel projections, and does not change Port Tunnel behavior. The public URL is composed as `<tunnel-url>/s/<share-token>/`, and the LAN URL is always shown alongside it.

An optional access password protects the whole share, not just the public origin. A present password must contain 1-128 characters and at least one non-whitespace character; it is hashed with SHA-256 and only the hash stays in memory. The share serves a bilingual password page, and a successful submission sets the `onespace_share` session cookie with `HttpOnly`, `SameSite=Strict`, a path scoped to `/s/<share-token>/`, and `Secure` when the request arrives over HTTPS. Failed submissions wait a fixed approximately 500 ms before responding and do not lock the share.

The public lifecycle is isolated from the LAN service. An initial public start checks for `cloudflared` before creating the share and rolls back a just-created share if the tunnel fails. A runtime public start checks the dependency first, leaves the running LAN share untouched on failure, records an error state, and allows a retry that creates a new tunnel id and URL. Stopping the share from the tool or tray, an unexpected listener exit, and application shutdown stop the share-owned tunnel; no share, token, password or public URL persists across restarts.

## Alternatives considered

- Reuse a saved Port Tunnel configuration: declined because the saved entry has a persistent identity and fixed local port, while a share has a random port, short lifetime and own token; coupling them would pollute the saved list and force users to manage a tunnel definition for each share.
- Support named Cloudflare tunnels, custom hostnames or FRP for File Sharing: declined because those modes require account or server setup and belong to Port Tunnel; Quick Tunnel provides the zero-configuration temporary link the feature needs.
- Determine protection from request origin headers or protect only the public URL: declined because both origins reach the same listener and origin headers are forgeable; one password session guards every request.
- Rely on an HTTP authentication prompt instead of a page and cookie: declined because the browser prompt is inconsistent across QR-opened mobile clients and offers no app-controlled session lifetime or bilingual guidance.
- Add a hard lockout or complex rate limiter for failed logins: declined because the share is short-lived and single-user; a fixed delay raises the cost of trivial guessing without adding shared state or a denial-of-service path against the owner.
- Fail the whole share when a runtime public start fails: declined because public exposure is optional and a tunnel failure must not interrupt LAN transfers.
- Persist the public URL and restore it after restart: declined because Quick Tunnel URLs are temporary and the share token, session and password are intentionally process-local.

## Consequences

- Public sharing requires `cloudflared`; a missing binary blocks an initial public start and surfaces install guidance while LAN-only sharing continues to work.
- The public URL is temporary and changes on every retry, while the LAN link remains plaintext HTTP, so the UI warns about internet exposure and about passwords or file content being observable on an untrusted LAN.
- File Sharing gains no new persistence and no migration; existing start calls without the new fields still create a LAN-only share, and Port Tunnel's saved list and runtime remain unchanged.
- Backend transition tests and frontend behavior tests cover URL composition, state mapping, rollback, toggle, retry, password validation and gating, event filtering and cleanup; live Quick Tunnel reachability remains a manual smoke check.
