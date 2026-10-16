# Maintained YAML dependencies

Issue [#19](https://github.com/reynholm/buzz/issues/19) removes the archived
`serde_yaml` / `unsafe-libyaml` graph reported by
[RUSTSEC-2024-0449](https://rustsec.org/advisories/RUSTSEC-2024-0449.html).
The advisory concerns maintenance; it does not identify a specific exploit.

Buzz's direct YAML consumers use `yaml_serde` 0.10.7. Both it and its
`libyaml-rs` 0.3 backend are maintained by the YAML Organization:
[yaml-serde](https://github.com/yaml/yaml-serde),
[libyaml-rs](https://github.com/yaml/libyaml-rs).
These are maintained forks, not a claim that the backend is safe Rust:
`libyaml-rs` retains translated unsafe code. We checked both dependency
manifests and exercised the actual Buzz consumers before removing the exception.
Dependency policy still applies to the replacement packages.

This choice preserves the existing generic YAML value model. In particular,
Desktop's legacy runtime rewrite retains unknown fields, custom tags, non-string
mapping keys, aliases' values, non-finite numbers and Markdown body content.
The rewrite already normalizes YAML formatting and expands alias references;
this change adds no bulk rewrite of existing files. Persona parsing, skill
discovery, workflow definitions and Goose config retain their existing scalar
coercion and error/fallback behavior. JSON workflow projections continue to reject
custom-tagged values rather than silently dropping their tags. Duplicate handling
remains target-dependent: typed structs and generic YAML mappings reject duplicate
keys, while JSON objects and HashMap targets retain the last value.

`serde-saphyr` was also evaluated. Its defaults differ for `on`/`no`, merge keys,
null-to-string conversion, custom tags and non-finite values, and it has no
drop-in generic YAML Value type. Replacing every direct consumer with it would
require an additional compatibility layer. That work is unnecessary for this
maintenance fix.

The Kubernetes backend uses upstream `kube` `~4.0` with `k8s-openapi` 0.28.
[Kube 4 removed the deprecated YAML dependency](https://github.com/kube-rs/kube/releases/tag/4.0.0)
in favor of `serde-saphyr`; kube 3.1 still depended on `serde_yaml`.
The minor constraint retains the workspace's declared Rust 1.88 minimum;
kube 4.2 raises it to 1.89. The provider explicitly retains kube 2's 295-second
read timeout and disables kube 4's new automatic retries, so a stalled response
or a long Retry-After cannot hide the API failure until the provider deadline.
The generated API schema changes from `v1_31` to `v1_32`, the earliest schema
available in OpenAPI 0.28. No runtime server-version check is added. Configuration
resolution remains KUBECONFIG → default kubeconfig, with the selected context,
namespace and exec plugin preserved. Tests exercise the actual `connect` and
exec lookup paths in isolated child processes, including a synthetic executable
credential plugin and requests to a local HTTP fixture. No live cluster or real
credentials are used. Live cluster acceptance remains a separate human check.

## Verification

- `cargo test --locked -p buzz-persona -p buzz-workflow -p buzz-agent -p buzz-backend-kubernetes`
- `cargo test --locked --manifest-path desktop/src-tauri/Cargo.toml --workspace`
- `python3 -m unittest discover -s scripts/fork/tests -v`
- `cargo tree --locked --target all -i yaml_serde`
- `cargo tree --locked --manifest-path desktop/src-tauri/Cargo.toml --target all -i yaml_serde`
- `cargo deny check` and the same command from `desktop/src-tauri`
- `just ci desktop-typecheck`

The unit CI recipe explicitly includes complete persona and workflow packages,
as well as the lockfile/advisory regression. Kubernetes and agent packages already
run there; Desktop's native workspace runs separately. Test fixtures are in the
actual consumer modules/packages, rather than a test of the parser alone.

For manual acceptance, load an existing persona pack and a skill with multiline
frontmatter, list a YAML workflow in Desktop, and read an existing Goose config.
For Kubernetes, use a test namespace and the same token/exec kubeconfig used
before the update; verify context selection and agent deployment. Do not use
production credentials in test fixtures. Report the cluster version tested.
