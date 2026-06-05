<div align="center">

# wisp

**Modern disk cleanup & analysis for Linux**

[![CI](https://github.com/chiiydd/wisp/actions/workflows/ci.yml/badge.svg)](https://github.com/chiiydd/wisp/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](#license)
[![MSRV](https://img.shields.io/badge/MSRV-1.90-orange)](rust-toolchain.toml)

A fast CLI + TUI for finding and freeing disk space — `pacman` cache, journal, orphans, browser caches, trash, dev caches (cargo / npm / pip / go / docker), and an interactive directory analyzer with a polar sector chart.

English · [中文](README.md)

</div>

---

## Features

- **One-shot cleanup** — `wisp clean` previews recommended user/dev cleanup by default; `wisp clean --apply` runs it.
- **Per-target cleaners** — pacman cache, paccache, orphans, journal, /tmp, trash, browsers, common desktop caches, thumbnails, cargo, npm, JavaScript toolchains, pip, Python tools, go, flatpak, docker.
- **Interactive TUI** — neovim-style chrome, mode badge, statusline hints; toggle bar chart / polar sector chart with `v`.
- **Safe by default** — three risk tiers (Safe / Moderate / Dangerous), explicit confirmations, hardcoded path blacklist, files moved to trash unless `--no-trash` (alias: `--purge`).
- **Streamable output** — `--output json|jsonl` for scripts, `human` for terminals.
- **Audit history** — every deletion is recorded with size, target, and timestamp; advanced users can query hidden `wisp history list` / `show`.
- **Advanced tooling** — shell completion, man pages, profiles, and state commands remain available, but are hidden from normal help until they are central to the workflow.

## Install

### From AUR (recommended on Arch)

```sh
# stable release
paru -S wisp
# or yay -S wisp

# bleeding edge (built from master)
paru -S wisp-git
```

### From source

Requires Rust 1.90+.

```sh
git clone https://github.com/chiiydd/wisp
cd wisp
cargo install --path crates/wisp-cli --locked
```

The binary lands at `~/.cargo/bin/wisp`.

### Pre-built binary

Grab the latest tarball from the [releases page](https://github.com/chiiydd/wisp/releases) and drop `wisp` into your `$PATH`.

## Quick start

```sh
wisp                       # launch TUI
wisp clean                 # preview recommended cleanup
wisp clean --apply         # apply recommended cleanup
wisp clean --deep          # preview high-risk cleanup too
wisp clean list            # advanced: list all cleaners
wisp analyze ~/Downloads   # one-shot analyzer (no TUI)
wisp doctor                # environment & permissions check
```

## Command surface

```
wisp
  ├─ tui                    # full-screen interface
  ├─ clean [--apply]        # preview recommended cleanup by default
  │   ├─ --deep             # include high-risk cleaners
  │   ├─ list               # advanced: list available cleaners
  │   └─ info <target>      # advanced: describe a single cleaner
  ├─ analyze [path]         # treemap / tree / flat views
  ├─ config info|show|edit  # config path, display, and editing
  ├─ doctor                 # diagnose
  └─ help                   # full command help
```

## TUI keys

| Key            | Action                                  |
| -------------- | --------------------------------------- |
| `j` / `k`      | move down / up                          |
| `h` / `l`      | back / enter                            |
| `⏎`            | activate selected item                  |
| `v`            | toggle bars ↔ polar sector chart        |
| `space`        | mark for deletion (analyzer)            |
| `d`            | delete marked entries                   |
| `q` / `Esc`    | quit / pop page                         |

The statusline mirrors the current mode, context (path / counts / viz), and a context-sensitive hint row.

## Architecture

Five-layer workspace, strictly one-way (L1 → L5):

| Layer | Crate            | Role                                                                                    |
| ----- | ---------------- | --------------------------------------------------------------------------------------- |
| L1    | `wisp-platform`  | Distro / package-manager / init-system traits. Arch impl today; extensible.             |
| L2    | `wisp-core`      | FS scanning, sizing, blacklist, trash, path safety. **Doesn't know what's being cleaned.** |
| L3    | `wisp-cleaners`  | Each cleaner declares *what* to clean; never touches FS directly. Auto-registered via `linkme`. |
| L4    | `wisp-engine`    | Builds `CleanPlan`, schedules execution, emits `ProgressEvent` stream, writes history.   |
| L5    | `wisp-cli` + `wisp-tui` | Thin presentation layer (clap / inquire / ratatui). Talks to the engine only.   |

See [docs/architecture.md](docs/architecture.md), [docs/cleaners.md](docs/cleaners.md), and [docs/adding-a-cleaner.md](docs/adding-a-cleaner.md).

## Safety model

- Hardcoded blacklist (`/`, `/etc`, `/usr`, `/home`, `/var`, …) — refused before reaching the executor.
- Three risk tiers; **Dangerous** cleaners are excluded from the recommended clean unless `--deep` is passed.
- Deletions go to trash by default; use `--no-trash` (alias: `--purge`) for permanent removal.
- `wisp clean` previews by default; pass `--apply` to execute. Legacy `-y` / `--yes` aliases still work.
- History is written to `~/.local/state/wisp/history.jsonl`; audit entries are written to `~/.local/state/wisp/audit.log`.

## Development

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
cargo deny check
```

CI runs fmt / clippy / test (stable, beta, MSRV 1.90) / cargo-deny on every push and PR.

See [CONTRIBUTING.md](CONTRIBUTING.md) for the layering contract, command-extension rules, and the "adding a cleaner" checklist.

## License

Licensed under the [MIT License](LICENSE) — see <https://opensource.org/licenses/MIT>.
