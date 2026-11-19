# Engineering decisions

## 2026-11-19 · Ring for outbound TLS

**Decision.** PDO and Redis use rustls with ring's default suites for TLS 1.2
and 1.3. The shared policy lives in `phera-io-pool`. The runtime registers ring
once for `mysql_async`, whose TLS builder takes the process-default provider.

**Why.** Ring is audited, widely deployed, and was already in the dependency
tree. It builds bundled C and assembly with the platform toolchain. Phera
continues to use safe Rust APIs.

**Rejected.** Keeping `rustls-rustcrypto 0.1.0-pre.0` would retain a pure-Rust
provider whose documentation labels it unsuitable for production. Removing it
also removes its exclusive dependency on `paste`.

## 2026-11-19 · RustSec advisory checks

**Decision.** The [Advisories job](../.github/workflows/ci.yml) scans the whole
lockfile on pull requests and on `main`. It fails on unlisted RustSec
advisories. Each entry in the [ignore file](../.github/advisory-ignores.json)
requires a reason.

**Why.** Explicit exceptions keep new advisories visible while giving existing
dependencies a migration path. RustSec flags `chrono` as unmaintained
(RUSTSEC-2026-0543). Replace it with a maintained date API while preserving
PostgreSQL date and timestamp decoding, HTTP cookie expiry formatting, and
session cookie and cache-header dates. Remove the exception once the lockfile
no longer contains `chrono`.

**Rejected.** A warning-only job or a blanket exemption for maintenance
advisories would let new findings pass without a documented decision. An
immediate `chrono` replacement would combine a date-compatibility migration
with the TLS provider change.
