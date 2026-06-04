# CLAUDE.md

Guidance for Claude Code working in this repository.

## What this project is

`wisp` is a Linux disk cleanup + analysis tool, shipped as one binary
(`wisp`) with both a CLI and a full-screen TUI. The codebase is a Rust 2024
workspace with a strict 5-layer architecture. Arch is the first fully-supported
distro; the L1 layer is built so Debian / Fedora / openSUSE can plug in without
touching higher layers (Phase 9 in `plan.md`). See [README.md](README.md) for
the user-facing tour and [docs/architecture.md](docs/architecture.md) for the
layer contract.

## Crate layout (memorise this — it's enforced)

```
L5  wisp-cli    +  wisp-tui    ← presentation (clap / inquire / ratatui)
L4  wisp-engine                 ← CleanPlan, scheduling, ProgressEvent stream
L3  wisp-cleaners               ← every "what to clean" lives here
L2  wisp-core                   ← FS scanning / trash / blacklist / safe paths
L1  wisp-platform               ← distro / package-manager / init traits
```

**Dependencies are strictly one-way**, low → high. `wisp-tui` may not import
`wisp-core` or `wisp-cleaners`; everything that L5 needs comes through
`wisp-engine`. A PR with a violating `use` statement is rejected by policy
(`CONTRIBUTING.md` §1) — don't propose one.

## Where things live

- **Cleaners** — `crates/wisp-cleaners/src/{system,user,dev}/<target>.rs`,
  one file per target. Each registers itself via `#[linkme::distributed_slice(CLEANERS)]`
  at the bottom of the file; the engine picks them up at startup. Adding a
  cleaner is a fixed checklist (see CONTRIBUTING.md §3 and
  `docs/adding-a-cleaner.md`).
- **Scanner / trash / blacklist** — `crates/wisp-core/src/{scanner,trash,fs}.rs`.
  Scanning uses `jwalk` + a dedicated rayon pool wrapped in
  `tokio::task::spawn_blocking`, so the engine sees `Future`s only.
- **Plan + execution** — `crates/wisp-engine/src/lib.rs` builds the
  `CleanPlan`, `audit.rs` writes the audit log, `history.rs` records sessions.
- **CLI command tree** — `crates/wisp-cli/src/cli.rs` is the single source of
  truth for subcommands (`clap` derive). Three output modes (`human`, `json`,
  `jsonl`) are mandatory for every command.
- **TUI pages** — `crates/wisp-tui/src/pages/{home,analyzer,cleaner,history}.rs`.
  Each page implements the chrome contract (`mode()`, `context()`, `hints()`)
  consumed by `app.rs` to render the title bar + statusline. Theme constants
  live in `theme.rs`; reusable key-hint type in `chrome.rs`.

## Hard rules when editing

These come from `CONTRIBUTING.md`. Treat them as blocking constraints, not style suggestions:

1. **Never violate the layer order.** If you reach for a path that crosses a
   layer (e.g. TUI calling a cleaner directly), stop and route through the
   engine instead.
2. **Use the approved CLI verbs** — `list / show / add / remove / clear / info
   / edit / set / get / reset / run / use / import / export`. No synonyms.
3. **Three output modes per command** — `human`, `json`, `jsonl`. If `jsonl`
   doesn't make sense for a one-shot result, fall back to `json`.
4. **Deletion code requires proptest coverage** for path traversal, symlinks,
   relative paths, and UTF-8 boundaries. `check_blacklist` and
   `check_no_traversal` must run before any FS mutation.
5. **Adding a Cleaner** is a fixed checklist: `CleanerMeta` impl,
   `CleanerExec::plan` (with dry-run path), `linkme` registration,
   `RiskLevel`, unit + proptest, `docs/cleaners.md` entry.
6. **No magic numbers / index-based dispatch in the TUI.** Use enums to carry
   semantics on menu items (see `pages/home.rs` and `pages/cleaner.rs` for the
   pattern). The user has explicitly asked for this.

## Conventions worth knowing

- **Async model is tokio-only.** Rayon is hidden behind `spawn_blocking`; never
  expose it across module boundaries.
- **Path types**: prefer `camino::Utf8PathBuf` over `std::path::PathBuf` —
  the workspace is UTF-8-only by design.
- **Errors**: `thiserror` for library-level errors, `color_eyre::Result` at
  binary boundaries.
- **Logging**: `tracing` with span names following the §7.6 hierarchy in
  `plan.md`. New cross-layer calls open a span.
- **Lints**: `[workspace.lints.clippy]` runs `pedantic` + `nursery` and warns
  on `unwrap_used` / `expect_used`. New code shouldn't introduce these.
- **MSRV is 1.90** — CI gates this; avoid features newer than that.
- **The release profile is `lto = "fat"`, `panic = "abort"`, stripped.** Don't
  rely on `panic = unwind` semantics anywhere.

## Common workflows

```sh
# Local validation before commit
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
cargo deny check

# Run the binary
cargo run --release -- clean @user -n
cargo run --release            # TUI

# Generate completions / man (used by packaging)
cargo run --release -- completion zsh
cargo run --release -- man
```

CI mirrors the first block on every push and PR.

## Releasing

Tag-driven. Push `vX.Y.Z` and `.github/workflows/release.yml` builds binaries
for `x86_64-gnu`, `x86_64-musl`, `aarch64-gnu` and attaches them to a GitHub
Release. AUR PKGBUILDs live in `packaging/aur/`; see `packaging/README.md` for
the push flow. The library crates are **not** published to crates.io — the
only outward artifact is the `wisp` binary.

## What's intentionally NOT in this repo

- No multi-distro support yet (Phase 9 in `plan.md`). Adding Debian / Fedora
  means new L1 + L3 implementations only — L2 / L4 / L5 must stay untouched.
- No telemetry / network calls. The audit log is local-only.
- No crates.io publishing. All wisp-* crates are workspace-internal.

## Reference docs

- [plan.md](plan.md) — full design doc, including phase plan and the §11 hard rules for agents.
- [docs/architecture.md](docs/architecture.md) — layer responsibilities and data flow.
- [docs/adding-a-cleaner.md](docs/adding-a-cleaner.md) — the cleaner checklist with code examples.
- [CONTRIBUTING.md](CONTRIBUTING.md) — review-time blocking rules.
