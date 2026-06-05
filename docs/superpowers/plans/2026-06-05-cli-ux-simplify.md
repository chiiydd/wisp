# CLI UX Simplification Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the command-line interaction simpler for normal users while preserving advanced commands and compatibility where practical.

**Architecture:** Keep the existing clap-based CLI and engine behavior. Move cleaning-specific flags from global help into `clean`, hide unfinished or advanced commands from ordinary help, and make `wisp clean` produce a safe recommended dry-run plan by default. `--apply` becomes the clear execution flag, with `-y`/`--yes` kept as clean-command aliases.

**Tech Stack:** Rust, clap, wisp-cli, wisp-engine, cargo fmt/clippy/test.

---

### Task 1: Simplify CLI Shape

**Files:**
- Modify: `crates/wisp-cli/src/cli.rs`
- Modify: `crates/wisp-cli/src/main.rs`

- [ ] **Step 1: Add failing CLI parser/help tests**

Add tests in `cli.rs` that assert:
- `wisp clean` parses without a target
- `wisp clean --apply` sets clean apply mode
- `wisp clean -y` still parses as apply
- root help hides `state`, `profile`, `completion`, and `man`
- clean help shows `--apply`

- [ ] **Step 2: Verify RED**

Run:

```sh
cargo test --package wisp-cli cli::tests --all-targets
```

Expected: fails because `CleanArgs.apply` does not exist and commands are still visible.

- [ ] **Step 3: Implement CLI shape**

Change `GlobalOpts` to only show generic flags:
- `--verbose`
- `--quiet`
- `--no-color`
- `--config`

Add these `CleanArgs` flags:
- `-a, --apply`, with aliases `-y` and `--yes`
- `-n, --dry-run`
- `--deep`
- `--no-trash`, alias `--purge`
- `--output human|json|jsonl`

Hide unfinished/advanced commands from root help:
- `history`
- `state`
- `profile`
- `completion`
- `man`

Hide unfinished subcommands:
- `history restore`, `history clear`
- `state fav add`, `state fav remove`, `state export`, `state import`
- `config set`, `config reset`

- [ ] **Step 4: Verify GREEN**

Run:

```sh
cargo test --package wisp-cli cli::tests --all-targets
```

Expected: tests pass.

- [ ] **Step 5: Commit**

```sh
git add crates/wisp-cli/src/cli.rs crates/wisp-cli/src/main.rs
git commit -m "codex fix: simplify cli help surface"
```

---

### Task 2: Make `wisp clean` Recommended Preview

**Files:**
- Modify: `crates/wisp-cli/src/main.rs`

- [ ] **Step 1: Add behavior tests or smoke commands**

Use command smoke tests after implementation because dispatch is async and integrated with engine state:

```sh
target/debug/wisp clean --output json | python3 -m json.tool
target/debug/wisp clean --output json | python3 -c 'import sys,json; data=json.load(sys.stdin); print(data["command"])'
target/debug/wisp clean --output json | python3 -c 'import sys,json; data=json.load(sys.stdin); print(any(r == "dangerous" for r in data["data"]["risks"]))'
```

Expected after implementation:
- JSON parses
- command is `clean recommended`
- dangerous risk presence is `False`

- [ ] **Step 2: Implement default recommended plan**

In `dispatch_clean`:
- If no target is supplied, build targets `["@user", "@dev"]`
- Filter `Dangerous` actions out unless `--deep` is supplied
- Default to dry-run unless `--apply` is supplied
- Use `--apply` as the execution switch

- [ ] **Step 3: Verify behavior**

Run:

```sh
cargo build --workspace
target/debug/wisp clean --output json | python3 -m json.tool
target/debug/wisp clean --output json | python3 -c 'import sys,json; data=json.load(sys.stdin); print(data["command"])'
target/debug/wisp clean --output json | python3 -c 'import sys,json; data=json.load(sys.stdin); print(any(r == "dangerous" for r in data["data"]["risks"]))'
target/debug/wisp clean --deep --output json | python3 -m json.tool
target/debug/wisp clean @user -n --output json | python3 -m json.tool
```

Expected:
- all commands exit 0
- default command label is `clean recommended`
- default plan has no dangerous actions
- explicit `@user` still parses and previews

- [ ] **Step 4: Commit**

```sh
git add crates/wisp-cli/src/main.rs
git commit -m "codex fix: make clean default to recommended preview"
```

---

### Task 3: Update User Docs And Verify

**Files:**
- Modify: `README.md`
- Modify: `README_EN.md`

- [ ] **Step 1: Update docs**

Present the normal workflow as:

```sh
wisp clean
wisp clean --apply
wisp clean --deep
wisp analyze ~
```

Mention `clean list` and `clean info` as advanced discovery commands.

- [ ] **Step 2: Final verification**

Run:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
cargo build --workspace
target/debug/wisp --help
target/debug/wisp clean --help
target/debug/wisp clean --output json | python3 -m json.tool
target/debug/wisp clean --apply --output jsonl | python3 -c 'import sys,json; [json.loads(line) for line in sys.stdin if line.strip()]'
```

Expected: all commands exit 0. Root help is short; clean help contains clean-specific flags.

- [ ] **Step 3: Commit**

```sh
git add README.md README_EN.md
git commit -m "codex fix: document simplified clean workflow"
```

---

## Self-Review

**Spec coverage:** This plan covers simplified top-level help, `clean` as the main workflow, `--apply`, safe default preview, hidden unfinished commands, and docs.

**Placeholder scan:** No placeholders or undefined steps remain.

**Type consistency:** The plan uses existing clap derive types, `CleanArgs`, `GlobalOpts`, `OutputFormat`, and `CleanPlan`.
