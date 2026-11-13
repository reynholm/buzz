# Release dependency security

The release dependency checks use the current RustSec advisory database for both
Cargo workspaces. A successful package build does not replace these checks:

```sh
cargo deny check
cargo deny --manifest-path desktop/src-tauri/Cargo.toml --target aarch64-apple-darwin --features mesh-llm check
cargo deny --manifest-path desktop/src-tauri/Cargo.toml --target x86_64-unknown-linux-gnu check
```

## Patched vulnerabilities

The emSign certificate-trust advisories
[RUSTSEC-2026-0381](https://rustsec.org/advisories/RUSTSEC-2026-0381.html),
[RUSTSEC-2026-0382](https://rustsec.org/advisories/RUSTSEC-2026-0382.html) and
[RUSTSEC-2026-0383](https://rustsec.org/advisories/RUSTSEC-2026-0383.html)
share CVE-2026-111943. Both lockfiles select `rustls-webpki` 0.103.16,
`webpki-root-certs` 1.0.10 and `webpki-roots` 1.0.10. The `webpki-roots`
0.26.11 compatibility wrapper remains: it contains no independent trust store
and now resolves to the fixed 1.0.10 parent. These compatible updates reject
the distrusted intermediate keys and remove the affected bundled root without
changing the existing immutable compatibility pins or advisory exceptions.

The affected scope is certificate verification by Rust clients when their
configured trust anchors permit the advisory's certificate chains, including
native WebSocket connections. Updating these dependencies does not update an
external proxy that terminates the relay's incoming TLS, and does not establish
that a deployment was compromised. Existing installations require rebuilt,
verified binaries and the normal acceptance and release authorization.

Compatible TLS security updates must proceed independently of upstream sync.
Before release, run the checks above against every actual shipping graph and
record the exact source commit, target, features and audit time. A passing
macOS/Linux audit does not certify Windows. No TLS advisory exception is added;
the existing quick-xml and INFO-only exceptions remain unchanged.

[Negentropy RUSTSEC-2026-0365](https://rustsec.org/advisories/RUSTSEC-2026-0365.html)
allows a malformed reconciliation message to request a large allocation before
its ID count is validated. Both lockfiles select the upstream 0.5.2 fix.

The ZIP extraction advisories
[RUSTSEC-2026-0367](https://rustsec.org/advisories/RUSTSEC-2026-0367.html),
[RUSTSEC-2026-0368](https://rustsec.org/advisories/RUSTSEC-2026-0368.html) and
[RUSTSEC-2026-0369](https://rustsec.org/advisories/RUSTSEC-2026-0369.html)
are fixed by ZIP 9. Buzz's native dependency and the MeshLLM compatibility pin
use 9.0.5. The pin preserves the v0.76.2 MeshLLM API: it updates the system and
plugin-manager ZIP dependencies and removes the unused host-runtime ZIP
reference. It does not change the mesh protocol or require a full MeshLLM
feature migration. All MeshLLM dependencies must resolve to the same immutable
fork commit to avoid duplicate protocol/runtime types. The current pin is
[`9e7c2fb2`](https://github.com/reynholm/mesh-llm/commit/9e7c2fb2181f66bea1f325c633e5a4364164d6fd),
based on upstream `a0c1e66b` (v0.76.2). Remove this pin when an
upstream release compatible with Buzz provides the patched ZIP and SQLite graph.

The native lockfile also retains ZIP 4 for the updater plugin on Windows.
That dependency is excluded from both requested macOS/Linux shipping graphs;
this release does not build or certify a Windows package. Updater ZIP migration
must be addressed before claiming a clean Windows release graph.

No exception is added for these four vulnerability advisories. A regression to
an affected dependency version must fail the normal advisory gate.

The native lock also updates `h2` to 0.4.16 and `rtrb` to 0.3.5, fixing
[RUSTSEC-2026-0258](https://rustsec.org/advisories/RUSTSEC-2026-0258.html) and
[RUSTSEC-2026-0274](https://rustsec.org/advisories/RUSTSEC-2026-0274.html).
NetEq 0.8.3 also declared an unused ringbuf dependency affected by
[RUSTSEC-2026-0293](https://rustsec.org/advisories/RUSTSEC-2026-0293.html).
The [compatibility pin](https://github.com/security-union/videocall-rs/commit/16729881b0a80022a7cbe0acc94c9becf2fad313)
removes only that manifest entry; the decoder/jitter-buffer source and API
remain identical to 0.8.3. Remove the pin when an upstream compatible NetEq
release drops the unused dependency.
The public source URL preserves the exact pinned commit and dependency contents.

Tauri 2.12.4 with utilities 2.10.2 removes the retired Unicode package graph via urlpattern
0.6. The coordinated Tauri 2 update is required because utilities 2.10 adds
configuration fields that the older Tauri test context cannot initialize.
Application directory overrides remain unset. Opus 0.4 replaces the retired audiopus_sys backend with opusic-sys.

The constant-time utility advisory
[RUSTSEC-2026-0385](https://rustsec.org/advisories/RUSTSEC-2026-0385.html)
is fixed by `ctutils` 0.4.4 or later; both lockfiles select 0.4.5. The patch registry
records this security floor rather than an exact compatibility pin, so later
compatible updates remain allowed. Earlier releases could generate
branches on secret values during mask generation and, on 32-bit targets,
`u128` comparisons. The compatible patch updates the version and checksum only;
no advisory exception is added. Run the full dependency policy for both shipping
workspaces after resolving against the current advisory database.

The P-256 App Attest dependency uses `p256` 0.14 and `crypto-bigint` 0.7.6
to address [RUSTSEC-2026-0387](https://rustsec.org/advisories/RUSTSEC-2026-0387.html).
The patched arithmetic removes secret-dependent branches. SEC1 point extraction
uses the current API names with the same uncompressed encoding; existing App
Attest certificate, authenticator and signed assertion tests exercise the
verification boundary. No advisory exception is added.

## DNS, SQLite and MCP bounds

Both workspaces select `hickory-proto` 0.26.2 to fix
[RUSTSEC-2026-0487](https://rustsec.org/advisories/RUSTSEC-2026-0487.html),
[RUSTSEC-2026-0488](https://rustsec.org/advisories/RUSTSEC-2026-0488.html),
[RUSTSEC-2026-0489](https://rustsec.org/advisories/RUSTSEC-2026-0489.html),
[RUSTSEC-2026-0490](https://rustsec.org/advisories/RUSTSEC-2026-0490.html) and
[RUSTSEC-2026-0491](https://rustsec.org/advisories/RUSTSEC-2026-0491.html).
These fixes bound DNS allocations and reject malformed DNS text and wire inputs.

The Mesh compatibility pin also moves its two SQLite consumers to `rusqlite`
0.41.1 with `libsqlite3-sys` 0.39.1, bundling SQLite 3.54.1 and addressing
[RUSTSEC-2026-0459](https://rustsec.org/advisories/RUSTSEC-2026-0459.html).
Its only additional changes are the two manifest requirements and their lockfile
entries; Mesh protocol and runtime sources remain unchanged. Buzz uses the same
immutable Mesh source in the native client and smoke harnesses. SQLx's optional
SQLite resolver constraint uses a minimal
[compatibility revision](https://github.com/reynholm/sqlx/commit/b159f0cc4779562bb997379aa60aeed680911840)
based on `c3735abf`: it changes only the optional SQLite upper bound to allow
the fixed 0.39 ABI. All seven SQLx crates share that source and retain the
whole-return deadline and cancelled-BEGIN rollback fixes. PostgreSQL remains
Buzz's enabled SQLx driver. Remove this revision only when a replacement
retains all three fixes.
The shipping Rust 1.98.1 toolchain satisfies the new SQLite dependency minimum.

Both graphs use `rmcp` 3.9.0, addressing
[RUSTSEC-2026-0457](https://rustsec.org/advisories/RUSTSEC-2026-0457.html),
[RUSTSEC-2026-0461](https://rustsec.org/advisories/RUSTSEC-2026-0461.html) and
[RUSTSEC-2026-0462](https://rustsec.org/advisories/RUSTSEC-2026-0462.html).
The agent retains child-process transport, discovery and call deadlines,
process-tree cleanup and bounded tool-result text/images through the new
`ContentBlock` API. The transport now rejects stdio lines above 16 MiB; regression
coverage distinguishes that boundary from the existing 128-tool metadata limit.
No vulnerability exception is added. Rebuilt native graphs and the complete
agent suite must be verified alongside the advisory checks above.

## Temporary transitive maintenance exceptions

The following three advisories identify standalone crates whose APIs have moved
into zbus 6, rather than a reported exploitable defect:

| Advisory | Retired standalone crate |
| --- | --- |
| [RUSTSEC-2026-0370](https://rustsec.org/advisories/RUSTSEC-2026-0370.html) | zvariant |
| [RUSTSEC-2026-0371](https://rustsec.org/advisories/RUSTSEC-2026-0371.html) | zbus_names |
| [RUSTSEC-2026-0372](https://rustsec.org/advisories/RUSTSEC-2026-0372.html) | zvariant_derive |

At the release recheck, maintained `notify-rust` 4.18.2, `ashpd` 0.13.13,
`secret-service` 5.2.0 and `zbus-secret-service-keyring-store` 1.0.1 still require
zbus 4/5 or its standalone type crates. Their manifests are available through
the [crates.io API](https://crates.io/data-access). The upstream migration is
tracked by [zbus issue 2043](https://github.com/z-galaxy/zbus/issues/2043).
Updating only Buzz's direct dependencies cannot eliminate the transitive types
without replacing the notification, portal and Secret Service integrations.

`deny.toml` therefore records these three INFO-only exceptions individually.
This retains the existing notification lifetime, native dialogs and encrypted
Secret Service behavior while upstream consumers migrate. It does not waive
future vulnerability advisories for these crates or change the policy for other
unmaintained dependencies. Recheck the parent manifests at each release and
remove each exception when the relevant graph no longer uses the retired crate.
The earlier unsafe-libyaml exception stays removed.

An additional INFO-only exception covers the macOS idle detector
dependency advisory, [RUSTSEC-2020-0168](https://rustsec.org/advisories/RUSTSEC-2020-0168.html),
for the legacy `mach` bindings behind current `user-idle` 0.6. There is no
compatible parent release using mach2. This INFO-only exception preserves the
idle detector; remove it when the parent updates or a separately validated
platform-binding migration replaces it. It does not waive future vulnerabilities.

Linux GTK3/WebKitGTK bindings similarly retain
[RUSTSEC-2024-0370](https://rustsec.org/advisories/RUSTSEC-2024-0370.html) for
their compile-time proc-macro-error dependency. The maintained replacement is
a different crate, proc-macro-error2; remove this INFO-only exception after the
GTK3 macro consumers migrate. It does not change application input parsing.

For a release review, inspect both complete dependency graphs, the exact pin
diff, native ZIP extraction tests, Linux notification/dialog/keyring startup,
and the packaged binaries. Automated and packaged startup checks do not claim
human acceptance of native dialogs or a real desktop Secret Service session.
