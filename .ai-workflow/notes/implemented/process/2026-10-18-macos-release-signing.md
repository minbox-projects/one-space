# Agent Note: macOS Release Signing Uses a Dedicated Self-Signed Certificate

Status: implemented

English | [中文](2026-10-18-macos-release-signing.zh.md)

## Problem

Each ad-hoc-signed macOS build had a designated requirement pinned to that build's cdhash. Because the cdhash changes with the code, macOS treated every release as a different application, so a user's keychain, Bluetooth and Accessibility grants were lost on each update and the prompts recurred. The release workflow also configured no Apple signing material, so a missing or misconfigured signing setup would silently produce an ad-hoc artifact.

## Decision

The repository signs every macOS release artifact with a dedicated self-signed code-signing certificate, `OneSpace Release Signing`, created once by `skills/onespace-release/scripts/create_signing_certificate.sh`. The certificate anchors the designated requirement to the certificate hash instead of the cdhash, so the requirement text is identical across builds and later signed updates keep the user's existing grants.

The bootstrap generates an RSA 2048 self-signed certificate with the `codeSigning` extended key usage and a ten-year validity, imports it for `codesign`, and writes `onespace-release-signing.p12`, `certificate-base64.txt` and `password.txt` (mode `0600`) for the GitHub secrets `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD` and `APPLE_SIGNING_IDENTITY`. It refuses to run again while the certificate exists, and the exported material must be backed up off the repository.

The release workflow fails the macOS jobs before any build when any signing secret is empty, imports the certificate into a per-run keychain and asserts the named identity is present, and after `tauri-action` verifies the produced bundle with `skills/onespace-release/scripts/verify_release_artifact.sh`, which requires `codesign --verify --strict` to pass, the identifier `com.onespace.desktop`, a non-ad-hoc signature, and a certificate-anchored designated requirement (accepting the root or leaf anchor form and rejecting cdhash).

## Alternatives considered

- Apple Developer ID signing with notarization: declined because it requires a paid Apple Developer account and notarization infrastructure; the free self-signed route gives the same cross-build permission stability for the project's non-App-Store distribution.
- Keep the ad-hoc status quo: declined because a cdhash-pinned requirement cannot be stable across builds, so users keep losing and re-granting permissions on every update.
- Regenerate the certificate per build or when convenient: declined because the certificate is the anchor that users' grants are bound to; regenerating it makes macOS treat the app as a new signer and invalidates every existing grant.

## Consequences

- Certificate-anchored requirements are stable across builds, so a signed update needs no re-grant; only the first signed release asks every existing user to confirm keychain, Bluetooth and Accessibility once, and that one-time notice belongs in the release body.
- Losing or regenerating the certificate is equivalent to requiring every user to re-grant all three permissions, so the p12 and its password must be backed up off the repository and the bootstrap is never re-run with `--force` after a release.
- The pre-build guard and the post-build verification step act as tripwires: an absent secret fails before building, and an ad-hoc or wrongly signed artifact fails verification.
- Rollback removes the guard, import and verification steps and the `APPLE_SIGNING_IDENTITY` environment entry; the certificate and secrets may stay, and the trade-off is the return of the recurring prompts.
