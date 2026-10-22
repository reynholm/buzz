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
[`eaa05df0`](https://github.com/reynholm/mesh-llm/commit/eaa05df056694754b6efbdab00f1e1754676008f),
based on upstream `a0c1e66b` (v0.76.2). Remove this pin when an
upstream release compatible with Buzz provides the patched ZIP graph.

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
