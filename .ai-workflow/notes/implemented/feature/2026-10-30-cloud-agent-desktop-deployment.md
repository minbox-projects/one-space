# Agent Note: Cloud Agent Desktop Deployment

Status: implemented

English | [中文](2026-10-30-cloud-agent-desktop-deployment.zh.md)

## Problem

Operators reach the Cloud Agent console over SSH, but before this change the console could only manage an Agent that was already installed; a first installation required running the server deploy script by hand from a source checkout. The bundled server package and the desktop application also had no shared statement of which Cloud Agent version was shipped, an existing host could not be classified as empty, partially published or complete before Python was available, and no single explicit action installed the Agent, selected that exact host and health-checked it afterwards.

## Decision

### Release identity and version sources

- `server/onespace-cloud-agent/VERSION` is the authoritative Cloud Agent release identity. The CLI deployer and the desktop application ship and deploy the same payload carrying that version, and the initial semantic version is a package choice independent of the OneSpace application and OpenCode versions.
- "Local latest" means the package bundled with the current OneSpace installation. No internet release lookup occurs, so a newer published release does not change what the console reports or uploads.
- Remote `VERSION` reports the deployed code version, while remote `install-info.json` carries `version` and a UTC ISO-8601 `installed_at`. They are independent files: installation metadata is written only after code, launcher and unit publication plus a verified active service, and a metadata version that disagrees with remote `VERSION` cannot supply the current installation timestamp.
- Unknown version and unrecorded time stay distinct. `VERSION` alone yields a version with an unrecorded time, neither file mtime nor the current time substitutes for installation time, and valid times render in the user's locale with an explicit timezone.

### Prerequisites

- New deployments require Linux, root access, an accessible systemd and amd64 or arm64.
- Ubuntu 22.04/24.04 or Debian 12 is required only when a runtime dependency is missing; when all five dependencies are already executable, another distribution satisfying the common platform requirements is accepted.
- Only missing `python3`, `git`, `tmux`, `gh` and `opencode` are installed, from official sources, and existing executables are preserved and never upgraded. GitHub authentication is not a deployment gate, and model credentials remain separately configured.

### Desktop handoff and deployment state

- Inspection uses shell-primitive file presence before Python is available and classifies the destination as installed, incomplete, not installed or probe error. A nonempty destination without a recognizable payload is a path conflict, and a configuration error is not evidence that the Agent is absent.
- A complete installation is never silently reinstalled. An incomplete destination can be retried only with in-process proof for the same host and installation path from an earlier failed deployment in this application process; that proof disappears on restart, after which manual repair is required.
- At most one deployment runs at a time. The dialog captures its target and locks the target and dismissal during mutation, consuming only matching operation progress; there is no cancellation API and no persisted job log, the deadline is 15 minutes, and application exit cleans up the local process group.
- The deploy command does not run an independent post-install status test. On success the console selects the captured target, invalidates previous host reads synchronously, suppresses polling during mutation and performs exactly one fresh status refresh, including when that target was already selected. A later failed test keeps the host selected with its installation information and offers read-only retest.
- Deployment never writes desktop Cloud Agent settings, never initializes a local settings file, never exports credentials and never changes the saved default host.

### Self-contained resources

- An installed application deploys without a checkout: an explicit 13-file `bundle.resources` allowlist containing the deployer, bootstrap script, `VERSION`, default and example configurations, `agent.py`, the six core Python modules and the systemd unit is resolved through Tauri resource paths with no checkout fallback.
- Tests, a local `config.json`, `.env` files and credentials are excluded from the bundle. Local payload and `VERSION` are validated before any SSH mutation, and the local version shown in the console is the version of the uploaded payload.

## Alternatives considered

- Add an upgrade and uninstall UI: declined because this change delivers first deployment only; existing installations stay read-only and selectable, and upgrade or removal remains an operator action.
- Build a persistent deployment-job engine: declined because one in-process deployment guard, matching progress events and a bounded deadline cover the single-operator case without a job store.
- Add a backend semantic-version comparator: declined because the frontend already owns display and comparison, and two comparators could disagree.
- Look up releases over the internet for "local latest": declined because the shipped package defines what this installation can deploy.
- Add automatic rollback or claim a rollback after timeout: declined because a timeout leaves remote state uncertain, so a later action inspects actual state instead of asserting recovery.
- Install prerequisites automatically on unsupported distributions: declined because their package sources and service management are not verified; the distribution gate applies only when a dependency is missing.

## Consequences

- The console gains a Deploy server entry, an installation summary showing remote version, local latest and installation time, a deployment dialog with target inspection and locked submission, and a read-only Retest action on the runtime card.
- The frontend owns semantic-version comparison and installed-time formatting. Malformed or absent versions produce an unknown comparison, while an optional `v` prefix, numeric and prerelease precedence and build-metadata equality follow semantic-version ordering.
- Legacy installations without `VERSION` or metadata remain installed and selectable with unknown version and unrecorded time, and missing historical metadata never authorizes redeployment.
- Failure preserves what already exists: a preflight, upload or publication failure leaves active files, configuration and snapshots intact, existing configuration stays byte-identical on retries, invalid JSON refuses publication, and OS packages installed before a failure are not rolled back.
- Known gaps: no real root-server deployment smoke was run; static resource inspection does not prove native GUI interaction; updater artifact signing requires the release key and was unavailable locally; partial OS package installs are not rolled back.
- Related notes: [Cloud Agent Hardening Guarantees](../architecture/2026-10-29-cloud-agent-hardening.md) records the service-side guarantees this deployment path reuses; this note partially supersedes its earlier deployment description while its validation, snapshot-restoration and compatibility rationale still stand.
