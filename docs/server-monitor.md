# Server Monitor

English | [中文](server-monitor.zh.md)

## Overview

Server Monitor collects CPU, memory, local disk and network usage from Linux hosts reachable over SSH, and shows current values, collection state and retained history in the established OneSpace tool style. It runs fixed, read-only commands with the SSH account you already use. It does not install an exporter or agent, open a metrics port, create a forwarding tunnel or change any remote configuration. The tool is Linux-only and requires direct SSH reachability from this computer; a host reachable only through another network needs its own access path.

## Requirements

- Direct SSH reachability from this computer, using the existing SSH behavior for configuration resolution, authentication and host-key verification.
- An existing SSH alias, or a custom connection with host, port, user and password or private-key path. Agent authentication is available through an existing SSH alias; custom connections offer password and key authentication.
- An ordinary user account that can read `/proc` and run `cat`, `sleep`, `df` and `uname`. `df` should be GNU-compatible because collection runs `LC_ALL=C df -l -P -k -T`.
- No `sudo`, installed service, exporter, agent, forwarding port or HTTP metrics endpoint is required. Missing or unsupported commands only degrade the affected resource.

## Managing targets

Use **Add** in the tool to create a target. A name is trimmed and must contain 1–80 characters. Choose an existing SSH alias or a custom connection; a custom connection requires a host, a user, an integer port from 1 to 65535 and either a password or a key path. Targets are enabled by default and may share a display name, but each target keeps its own identity.

**Edit** updates the same stored record. Leaving the password empty while editing a password target keeps the stored password and only reports that a password is present; entering a new password replaces it. A new password-authenticated target requires a password. **Test connection** performs one temporary one-shot collection with the draft values, closes its session on every outcome, and saves no target and no history. **Save** publishes the new state only after the encrypted configuration write succeeds.

**Pause** stops automatic collection for that target and closes its owned sessions; an explicit **Collect now** still works and leaves the target paused. **Delete** asks for confirmation and explains that the target's collected history is removed too. Confirming removes that target's samples and daily slot claims; cancelling writes nothing. Deleting one target never affects another target.

## Scheduling modes

Only one schedule mode is active at a time. **Settings → Services → Server Monitor** controls both modes, and the tool's settings action opens this section directly.

### Interval mode

Interval mode keeps at most one persistent SSH session per target. It collects on startup, target activation, resume and enabling automatic collection, then starts the next collection when the previous one finishes. An explicit 30-second idle keepalive is sent on the persistent session, and keepalive and collection are serialized on that session. Saving a changed interval rearms it from the save time without an extra collection. A manual collection that collides with a scheduled collection shares the in-flight result. At most four collections run at once, and one target never has more than one collection in flight.

### Daily mode

Daily mode accepts multiple local times. Each due collection opens a fresh one-shot SSH session, runs one collection and closes the session after success or failure; no session or keepalive remains between times. Startup, saving, target creation and resume arm only future times in this computer's timezone. A missed time is skipped and is not replayed after sleep or application exit.

Before any network work for a target, local date and time, the scheduler durably claims that slot. A slot is attempted at most once even across repeated ticks, application restarts or clock rollback, and a failed claim write starts no connection. A nonexistent spring-forward time is skipped; a repeated fall-back time runs at most once for that local date and time. A manual collection that coincides with a due slot claims that slot and shares its result. Switching interval → daily closes the persistent session and waits for the next future time; switching daily → interval performs the initial interval collection after old work is released. Global disable or target pause closes owned sessions.

## Settings and validation

Defaults are automatic collection enabled, interval mode, a 5-minute interval, daily times `09:00`, and 7-day history retention.

- Interval must be an integer from 1 to 1440 minutes.
- Retention must be an integer from 1 to 30 days.
- Daily times must be a nonempty list of exact 24-hour `HH:mm` values, without duplicates. `00:00` and `23:59` are valid; `9:00` and `24:00` are not. Valid saved times are sorted.

The frontend and backend both validate the submitted values and never silently correct an invalid submission. Only the active mode controls scheduling, while the inactive mode's values are retained. Save, reset and dirty state are local to this settings section and do not change other sections. Reset discards the draft and reloads the saved values. Shortening retention permanently removes history outside the new window; the section warns about this.

## Metrics and states

CPU comes from two `/proc/stat` samples taken inside one collection. Total time is user, nice, system, idle, iowait, irq, softirq and steal; guest and guest_nice are not added again. Usage is `100 × (Δtotal − Δidle − Δiowait) / Δtotal`. A negative delta, changed CPU identity or nonpositive total delta makes the CPU value unavailable.

Memory uses `/proc/meminfo`: used is `MemTotal − MemAvailable`, converted from kernel kB to 1024-byte units. A missing `MemAvailable` makes memory unavailable rather than falling back to `MemFree`.

Disk data comes from `LC_ALL=C df -l -P -k -T`. Each local persistent mount shows total, used and available space and `used / total`. Mount paths containing spaces are supported. `proc`, `sysfs`, `tmpfs`, `devtmpfs`, `overlay` and `squashfs` are excluded; mounts are never summed. A negative available count means zero usable space while the reported used and total values are preserved, and an invalid or zero total is unavailable. The disk card names the mount with the highest usage.

Network rates are computed per interface from counter deltas divided by the actual positive uptime difference within the same collection. `lo` is excluded and interfaces are never summed. The default is the first valid interface in name order, and the interface selector changes both the label and values. A changed boot identity, a missing or changed interface or decreasing counters makes the affected rates unavailable. CPU and network values cover the approximately one-second sampling window; memory and disk are snapshots.

An unavailable value is shown as `—`, never as zero. A collection with at least one available resource is partial; a non-Linux host, no usable resource, a script failure or an overall timeout fails the collection. Authentication, host-key and configuration errors are blocked states until an explicit retry or edit. A failed collection keeps the last good snapshot visible with its timestamp and error, daily mode reports waiting between times, and a disabled target reports paused.

## History

History uses actual collection timestamps on the 24-hour and 7-day axes. Daily observations are discrete points, not an implied continuous measurement. Interval lines break across failures, pauses and gaps longer than twice the interval recorded with those observations, so a sleep gap or a stopped schedule is visible instead of being bridged. History is clipped to the retained window.

The database retains samples according to the retention setting. Pruning on startup and after collections removes old samples but keeps today's and already claimed future daily slots, so a retention change or pruning cannot cause today's slot to run again. Deleting a target removes that target's samples and claims only.

## Privacy and local storage

The feature stores data in the active profile's application directory: encrypted `server_monitor.enc.json` for targets and settings, and a local `server_monitor.db` SQLite database for samples and daily slot claims. Configuration is encrypted with the application master password, written atomically, and never overwritten when an existing document cannot be read or decrypted. Passwords stay inside the encrypted record; snapshots, history, events and logs expose only whether a password is present. These files are not added to iCloud or Git synchronization, and no credential is sent anywhere except the target's SSH authentication.

## Troubleshooting and rollback

- **Configuration unreadable** — the app reports it and refuses to start collection or overwrite the file. Fix or remove the file yourself before retrying.
- **Authentication, host key or configuration error** — the target is blocked. Correct the connection or use an explicit retry/resume; these errors do not repeatedly authenticate in a loop.
- **No usable resources, script failure or timeout** — confirm the host is Linux, `/proc` is readable by the ordinary account, and `cat`, `sleep`, `df` and `uname` are available.
- **Waiting between daily times** — an intentionally closed session is not an offline server; the next future time will connect.
- **Release sessions** — quit the app or disable automatic collection and pause targets; application shutdown closes owned sessions. Stopping the feature/application is sufficient to release its sessions.
- **Rollback** — reverting the feature's implementation leaves `server_monitor.enc.json` and `server_monitor.db` untouched; older application versions ignore both files. No remote configuration, service or installed software has to be removed. Reinstalling the feature reopens its own stored targets and history, and every collection takes a fresh sample pair.

## Verification status

Behavior tests cover the collector parser and calculations, session ownership and lifetime, storage and slot claims, scheduling, commands and the tool/settings interactions. A controlled real SSH/Linux smoke that exercises repeated interval collections on one authenticated session and daily one-shot closure was **not performed** for this delivery, and native visual inspection in the packaged application was **not performed**. Both remain unverified manual gaps; fixtures and jsdom tests are not a substitute for them.
