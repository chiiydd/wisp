<div align="center">

# wisp

**Modern disk cleanup & analysis for Arch Linux**

[![CI](https://github.com/chiiydd/wisp/actions/workflows/ci.yml/badge.svg)](https://github.com/chiiydd/wisp/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)
[![MSRV](https://img.shields.io/badge/MSRV-1.90-orange)](rust-toolchain.toml)

A fast CLI + TUI for finding and freeing disk space — `pacman` cache, journal, orphans, browser caches, trash, dev caches (cargo / npm / pip / go / docker), and an interactive directory analyzer with a polar sector chart.

</div>

---

## Features

- **One-shot cleanup** — `wisp clean @user`, `@system`, `@dev`, `@all` (dry-run by default).
- **Per-target cleaners** — pacman cache, paccache, orphans, journal, /tmp, trash, browsers, thumbnails, cargo, npm, pip, go, flatpak, docker.
- **Interactive TUI** — neovim-style chrome, mode badge, statusline hints; toggle bar chart / polar sector chart with `v`.
- **Safe by default** — three risk tiers (Safe / Moderate / Dangerous), explicit confirmations, hardcoded path blacklist, files moved to trash unless `--purge`.
- **Streamable output** — `--output json|jsonl` for scripts, `human` for terminals.
- **Audit history** — every deletion is recorded with size, target, and timestamp; `wisp history list` / `restore`.
- **Cross-shell completion + man page** — `wisp completion zsh`, `wisp man`.

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
wisp clean @user -n        # preview user-scope cleanup
wisp clean pacman -y       # apply pacman cache cleanup
wisp analyze ~/Downloads   # one-shot analyzer (no TUI)
wisp doctor                # environment & permissions check
wisp history list          # past clean sessions
```

## Command surface

```
wisp [--output human|json|jsonl]
  ├─ tui                    # full-screen interface
  ├─ clean <target> [-n|-y] # @all · @system · @user · @dev · or per-target
  │   ├─ list               # list available cleaners
  │   └─ info <target>      # describe a single cleaner
  ├─ analyze [path]         # treemap / tree / flat views
  │   └─ cache list|clear   # saved scans
  ├─ history list|show|restore|clear
  ├─ state fav add|list|remove · path · export · import
  ├─ config show|edit|set|reset
  ├─ profile list           # named cleanup profiles
  ├─ doctor                 # diagnose
  ├─ completion <shell>     # bash · zsh · fish · powershell · elvish
  └─ man                    # generate man page
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

See [docs/architecture.md](docs/architecture.md) and [docs/adding-a-cleaner.md](docs/adding-a-cleaner.md).

## Safety model

- Hardcoded blacklist (`/`, `/etc`, `/usr`, `/home`, `/var`, …) — refused before reaching the executor.
- Three risk tiers; **Dangerous** cleaners require an explicit `--yes` even after confirmation.
- All deletions go to trash by default; `--purge` for permanent removal.
- `--dry-run` (alias `-n`) is the default for `clean`; you must pass `-y` / `--yes` to apply.
- Audit log written to `~/.local/state/wisp/history.jsonl`.

## Development

```sh
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test  --all-targets --all-features
cargo deny check
```

CI runs fmt / clippy / test (stable, beta, MSRV 1.90) / cargo-deny on every push and PR.

See [CONTRIBUTING.md](CONTRIBUTING.md) for the layering contract, command-extension rules, and the "adding a cleaner" checklist.

## License

Dual-licensed under either of:

- Apache License 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT license       ([LICENSE-MIT](LICENSE-MIT)        or <https://opensource.org/licenses/MIT>)

at your option.

---

<details>
<summary><b>中文说明</b></summary>

`wisp` 是一个面向 Arch Linux 的现代化磁盘清理与分析工具，兼具命令行批量清理与全屏 TUI 交互式分析两种形态。

### 特性

- **一键清理**：`wisp clean @user / @system / @dev / @all`，默认 dry-run。
- **分目标清理器**：pacman、paccache、orphans、journal、/tmp、trash、浏览器缓存、缩略图、cargo / npm / pip / go / flatpak / docker。
- **交互 TUI**：neovim 风格 chrome、模式徽章、状态栏提示；按 `v` 切换柱状图 / 极坐标扇形图。
- **默认安全**：三档风险（Safe / Moderate / Dangerous）、显式确认、硬编码路径黑名单、删除走回收站，`--purge` 才永久删除。
- **流式输出**：`--output jsonl` 给脚本消费，`human` 给终端阅读。
- **审计与回退**：所有删除写入历史，可 `wisp history restore <id>` 回退（限回收站项）。
- **shell 补全 + man page**：`wisp completion zsh`、`wisp man`。

### 安装

```sh
# Arch / AUR
paru -S wisp        # 稳定版
paru -S wisp-git    # 跟随 master

# 源码
cargo install --path crates/wisp-cli --locked
```

### 快速上手

```sh
wisp                       # 进入 TUI
wisp clean @user -n        # 预览用户态清理
wisp clean pacman -y       # 真正执行 pacman 缓存清理
wisp analyze ~/Downloads   # 一次性分析（不进 TUI）
wisp doctor                # 环境/权限检查
wisp history list          # 历史清理记录
```

更多命令、TUI 按键、架构与安全模型见上文英文小节。

</details>
