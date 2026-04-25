# Contributing to wisp

## Hard constraints (PRs that violate these are rejected outright)

### 1. Layered architecture

wisp uses a strict 5-layer architecture.  **No upward or skip-layer dependencies.**

```
L5 wisp-tui / wisp-cli      ← presentation
L4 wisp-engine              ← orchestration
L3 wisp-cleaners            ← "what to clean"
L2 wisp-core                ← "how to safely scan/delete" + shared types
L1 wisp-platform            ← OS / distro abstraction
```

If any `use` statement in a PR violates this single-direction dependency order,
the PR will be rejected.  L5 may **not** directly call L2 or L3; everything
goes through the Engine.

### 2. New commands

New top-level commands or command domains require a GitHub issue with an RFC
before implementation.  All new functionality must fit into an existing command
domain (see Section 4 of the design doc).

**Approved verb vocabulary** (no synonyms):
`list` / `show` / `add` / `remove` / `clear` / `info` / `edit` / `set` / `get`
/ `reset` / `run` / `use` / `import` / `export`

### 3. New cleaner checklist

Every new `Cleaner` PR must include all of the following:

- [ ] `CleanerMeta` implementation (id, name, description, risk, group, root, supported_on)
- [ ] `CleanerExec::plan` implementation with dry-run path
- [ ] `#[linkme::distributed_slice(CLEANERS)]` registration entry
- [ ] `RiskLevel` declaration matching the cleaner's actual risk
- [ ] At least one unit test
- [ ] At least one `proptest` covering path injection / symlink / relative paths
- [ ] `docs/cleaners.md` entry updated

### 4. Security / deletion PRs

Any PR that introduces or modifies deletion logic **must** include:

- `proptest` coverage for: path traversal (`..`), symlink targets, relative paths, UTF-8 boundaries
- Proof that `check_blacklist` and `check_no_traversal` are called before any filesystem mutation

### 5. Performance-critical PRs

PRs touching the scanner hot path must include a `criterion` before/after
comparison in the PR description.

### 6. New dependencies

Before adding any new crate:

1. Explain in the PR description why `std` or existing workspace deps are insufficient.
2. Ensure the crate passes `cargo deny check`.
3. No crates with GPL/LGPL/AGPL licenses.
4. C-extension crates are discouraged (mimalloc is the only approved exception).

### 7. Tracing / logging

Every cross-layer call must open a `tracing::span!` following the hierarchy in
Section 7.6 of the design doc:

```
wisp.run → wisp.plan → cleaner.<id>
wisp.run → wisp.confirm
wisp.run → wisp.execute → action.<id> → fs.<op>
```

### 8. Output format

Every new command must support all three output formats:

| `--output` | Format |
|------------|--------|
| `human`    | human-readable text |
| `json`     | `OutputEnvelope<T>` JSON object |
| `jsonl`    | streaming `ProgressEvent` per line (fallback to `json` if not streamable) |

### 9. Exit codes

| Code | Meaning |
|------|---------|
| 0    | Success |
| 1    | Generic error |
| 2    | User cancelled |
| 3    | Insufficient privileges |
| 4    | Partial failure |
| 64   | Usage error |
| 70+  | Internal error |

### 10. Argument naming convention

| Concept | Argument name |
|---------|---------------|
| File path | `path` |
| Record ID | `id` |
| Clean target | `target` |
| Item count | `limit` |
| Byte size | `size` |

Do not introduce synonyms.

---

## Development workflow

```bash
# Full check (mirrors CI)
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets

# Quick iteration
cargo build
cargo run -- doctor
cargo run -- clean list
```
