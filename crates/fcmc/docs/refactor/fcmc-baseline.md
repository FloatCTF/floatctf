# fcmc Refactor Baseline

## Recording Time

2026-07-28

## Git State

- Branch: `awd`
- Status: clean (only `.codegraph/` and `PROJECT_AUDIT.md` untracked)
- Commit: `59180f2 refactor(fcmc): separate metadata and runtime; drop unused target`

## Test Results

```
cargo test --all-targets

running 9 tests (lib)
FAILED: metadata::gamebox::tests::test_create_and_start
  → DockerResponseServerError { status_code: 404, message: "No such image: floatctf/hello-floatctf:gamebox-web_v1.0.0" }
PASSED (8):
  - metadata::gamebox::tests::test_default_resource_limits
  - metadata::gamebox::tests::test_parse_new_integer_score_fields
  - metadata::gamebox::tests::test_parse_old_float_score_fields
  - metadata::gamebox::tests::test_parse_with_judge_config
  - metadata::gamebox::tests::test_reject_fractional_score
  - metadata::gamebox::tests::test_serialize_uses_new_field_names
  - runtime::model::tests::container_filter_to_bollard_labels
  - runtime::model::tests::network_spec_defaults_check_duplicate

running 1 test (integration)
FAILED: tests/test_build.rs::main
  → Os { code: 2, kind: NotFound, message: "No such file or directory" }
  (hardcoded Windows path: "E:/fb0sh/MyProjects/floatctf/fcmc/challenges/comment/meta.toml")
```

**Summary**: 8 passed, 2 failed (1 Docker-dependent, 1 hardcoded path)

## Docker Environment Dependency

- `test_create_and_start` requires local Docker daemon with image `floatctf/hello-floatctf:gamebox-web_v1.0.0`
- `tests/test_build.rs` requires Docker + hardcoded Windows path (always fails on macOS)

## Compiler Warnings (5)

All in `src/runtime/docker.rs:464-474`:
- Use of deprecated `bollard::container::LogsOptions`
- Use of deprecated fields: `stdout`, `stderr`, `tail`

## Clippy Warnings (12)

| Lint | File:Line | Description |
|---|---|---|
| `deprecated` | `runtime/docker.rs:464` | Deprecated LogsOptions struct |
| `deprecated` | `runtime/docker.rs:471` | Deprecated LogsOptions usage |
| `deprecated` | `runtime/docker.rs:472-474` | Deprecated fields stdout/stderr/tail |
| `ptr_arg` | `metadata/challenge.rs:94` | `&PathBuf` instead of `&Path` |
| `ptr_arg` | `metadata/gamebox.rs:470` | `&PathBuf` instead of `&Path` |
| `useless_format` | `metadata/gamebox.rs:236` | `format!("{}", identifier)` |
| `needless_update` | `metadata/gamebox.rs:276` | `..Default::default()` no effect |
| `collapsible_if` | `runtime/docker.rs:360-368` | Nested if-let can be collapsed |
| `collapsible_if` | `runtime/docker.rs:361` | Nested if-let can be collapsed |
| `collapsible_if` | `runtime/docker.rs:362` | Nested if-let can be collapsed |
| `let_unit_value` | `tests/test_build.rs:16` | Let binding unit value |

## fmt Check

```
cargo fmt --check → Clean (no output)
```

## Public Exports (lib.rs)

```rust
// AWD types (from runtime::awd)
pub use runtime::awd::{
    AwdContainerRuntime, ContainerHandle, ContainerState, DockerRuntime, EventNetworkSpec,
    GameBoxResetSpec, GameBoxSpec, InfrastructureContainerSpec, NetworkHandle, NetworkState,
    awd_labels,
};

// Unified runtime (from runtime)
pub use runtime::{
    ContainerFilter, ContainerRuntime, ContainerSpec, DEFAULT_STOP_TIMEOUT, DockerContainerRuntime,
    IMMEDIATE_STOP_TIMEOUT, NetworkInspect, NetworkSpec, PortBinding, ResourceLimits,
};

// Metadata
pub use metadata::{
    ChallengeMeta, DockerMeta, FlagMeta, GameBoxConfig, GameBoxMeta, HealthcheckConfig,
    JudgeCheckConfig, ResourceConfig,
};
```

**Duplicate type names**: `ContainerHandle`, `ContainerState`, `NetworkHandle`, `NetworkState` exist in both `runtime::model` and `runtime::awd`.

## Runtime Exports (runtime/mod.rs)

```rust
pub use docker::{ContainerRuntime, DockerContainerRuntime};
pub use model::{
    ContainerFilter, ContainerSpec, DEFAULT_STOP_TIMEOUT, HealthcheckSpec, IMMEDIATE_STOP_TIMEOUT,
    NetworkInspect, NetworkSpec, PortBinding, ResourceLimits,
};
pub use model::{ContainerHandle, ContainerState, NetworkHandle};
```

## CLI Contract

```
fcmc check [-p <path>]
  → Reads meta.toml, validates config, optionally tests Docker

fcmc build [-p <path>] [-f challenge|gamebox|target]
  → Builds Docker image from meta.toml

fcmc gen -n <name> [-o <output>] [-f challenge|gamebox|target] [-t]
  → Generates template directory with meta.toml + src/
```

**Known issues**:
- `target` format variant exists but `todo!()` in gen and build
- `check` requires user to press Enter to cleanup container (stdin dependency)

## Code Statistics

- Total lines: 2,224
- Files: 9 source files, 1 test file
- `src/main.rs`: 218 lines
- `src/lib.rs`: 27 lines
- `src/metadata/challenge.rs`: 214 lines
- `src/metadata/gamebox.rs`: 688 lines
- `src/runtime/model.rs`: 176 lines
- `src/runtime/docker.rs`: 487 lines
- `src/runtime/awd.rs`: 397 lines
- `src/metadata/mod.rs`: 9 lines
- `src/runtime/mod.rs`: 13 lines
- `tests/test_build.rs`: 19 lines

## Dependencies

```toml
tokio = { version = "1", features = ["full"] }
bollard = "0.19.1"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
toml = "0.9.2"
clap = { version = "4.5.16", features = ["derive"] }
anyhow = "1.0"
thiserror = "2.0"
uuid = { version = "1.0", features = ["serde", "v4"] }
colored = "2.1.0"
tar = "0.4"
tempfile = "3.23.0"
futures-util = "0.3.31"
tracing = "0.1"
async-trait = "0.1"
tokio-util = { version = "0.7.16", features = ["io"] }
```
