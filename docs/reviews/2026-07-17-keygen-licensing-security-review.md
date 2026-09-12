# Security re-review: Keygen licensing implementation

**Snapshot:** local workspace as of 2026-07-17 after W1–W6 implementation  
**Decision:** readiness to enter an isolated Keygen CE staging environment  
**Basis:** licensing architecture plan, official Keygen API contracts, fail-closed/security boundaries, Windows behavior, test/lint/dependency evidence  
**Scope:** `rust/src/licensing`, license UI integration, `tools/license-admin`, relevant Cargo manifests and lockfiles  
**Excluded:** live Keygen CE behavior, production infrastructure, legal approval, Authenticode/installer, independent penetration test

## Verdict

**Ready for staging with reservations. Not ready for production.**

No unresolved critical or high finding was demonstrated in the Windows licensing paths. Four implementation findings discovered during this self-review were corrected and re-tested. Production remains blocked by missing staging E2E evidence, unresolved business/infrastructure decision gates, signing/installer work, and technical debt in the pre-existing `three-d`/`cgmath` rendering stack.

## Finding disposition

| ID | Severity / confidence | Finding | Disposition and evidence |
|---|---|---|---|
| R1 | Major / certain | HTTP 5xx and 429 during refresh discarded a still-valid signed offline lease. | **Resolved.** Transient server responses now enter `ServiceUnavailable` with the unchanged bounded lease; 4xx validation/security errors remain fail-closed. A regression test distinguishes 503/429 from 403. |
| R2 | Moderate / certain | A disconnected background worker could leave `LicenseManager` permanently busy. | **Resolved.** `TryRecvError::Disconnected` clears the operation, preserves an existing lease where applicable, and schedules a retry; activation remains denied. Regression test added. |
| R3 | Major / high | Concurrent admin CLI processes could append two entries with the same audit predecessor and corrupt the chain. | **Resolved.** The CLI now holds an exclusive OS file lock for the full operation and verifies the chain after acquiring it. |
| R4 | Major / certain | An API mutation ran before the first durable audit write, allowing an operation without evidence if the result write failed. | **Resolved.** A synced `attempt` record is mandatory before the API call; a separate `result` record follows it. HMAC-chain tamper test passes. |
| R5 | Moderate / high | Authorization headers were not marked sensitive in HTTP header values. | **Resolved.** Both license and Bearer headers use `HeaderValue::set_sensitive(true)`; source and artifact scans found no embedded token/private-key value. |
| R6 | Moderate / high | The Windows release graph contains unmaintained `cgmath 0.18` through `three-d`; RustSec reports an unsound `swap_columns` implementation. | **Open, production owner.** Registry/source search found no call to `swap_columns` in the application or `three-d`, so no reachable failure was demonstrated. Replace or upgrade the 3D stack before production rather than carrying this dependency indefinitely. |
| R7 | Blocker for production / certain | No live staging account, IDs, public key, policy, license, or server was available. | **Open, infrastructure owner.** Scenarios activation → offline → refresh → suspend → reinstate → deactivate and operator commands remain unverified against Keygen CE. |
| R8 | Major / certain | Closed states with a stored license could not trigger a recovery refresh from the UI or periodic poller. | **Resolved.** Stored-license recovery states now expose online recovery in the license page, and the periodic poller may schedule a refresh for recoverable blocked states. Regression tests cover the manager/UI gate. |
| R9 | Major / certain | A stale refresh could clear a newer `authoritative_block` or `deactivation_pending` marker written by another process. | **Resolved.** Refresh/restore now preserve newer persisted state for the same binding, publish the actually committed record, and refuse to overwrite newer blocking markers. Regression tests cover both block and pending races. |
| R10 | Major / high | New security fields were stored under schema v1, so rollback to an older binary could ignore them and reopen offline access. | **Resolved.** Storage migrated to schema/envelope v2, legacy v1 records are upgraded in place on load, and the v1 envelope is rejected by the normal decoder. Regression tests cover both rejection and migration. |
| R11 | Moderate / certain | GitHub Actions workflow references used mutable refs instead of pinned commit SHAs. | **Resolved.** The Rust CI workflow now pins `actions/checkout`, `dtolnay/rust-toolchain`, and `Swatinem/rust-cache` to exact commits. |

## Controls confirmed

- HTTPS-only client, redirects disabled, bounded connect/total timeouts and response sizes.
- Mandatory Ed25519 verification of Keygen response target/host/date/digest/raw body before JSON parsing.
- Exact account/product/policy/license/machine/fingerprint assertions for encrypted signed machine files.
- Bounded offline lease, clock rollback detection, periodic online refresh and immediate fail-closed behavior for signed suspension/expiry responses.
- DPAPI CurrentUser storage, authenticated envelope, atomic replacement and secret zeroization at owned-string boundaries.
- Hashed multi-component hardware identity with conservative majority recovery.
- No admin token in the client; admin CLI is a separate Cargo package and reads secrets only at runtime.
- UI hides all functional pages until the central gate grants access; network work remains off the UI thread.

## Verification evidence

- Client: 62 unit tests passed, 2 explicit real-GL tests remained ignored, and all integration suites passed (31 tests total).
- Admin CLI: 9/9 tests passed, including confirmation, recursive redaction, migration and audit tamper/truncation detection.
- `cargo clippy --all-targets -- -D warnings`: passed for both packages.
- Native 460×600 license screen rendered and remained intact after moving the Windows window.
- RustSec: admin lockfile has no vulnerabilities. The client lockfile keeps informational `cgmath`/font-stack warnings, while `quick-xml 0.39.4` and `memmap2 0.5.10` are proven absent from the Windows target graph before their narrow audit exceptions are applied.
- Secret-pattern scan found only environment-variable names and documentation placeholders, not credential values.

## Remaining staging matrix

1. Provision Keygen CE staging with policy authentication `LICENSE` or `MIXED`, `maxMachines=1`, required fingerprint/components scope, check-in requirements and 7-day machine-file TTL.
2. Build the client with staging public configuration; never embed an admin/product token.
3. Exercise architecture scenarios 1–20, especially duplicate activation recovery, HTTP 5xx with a valid lease, clock rollback, hardware change and remote suspend.
4. Run admin acceptance with a least-privilege product token, a protected audit location and backup/restore of the audit key.
5. Resolve `three-d`/`cgmath`, run an independent security review, Authenticode-sign the client/installer and rehearse rollback before production.

## Review limitations

This is a self-review, not an independent penetration test or legal/security certification. Cryptographic tests use synthetic fixtures; only a live staging server can confirm self-hosted Keygen configuration and exact operational permissions.
