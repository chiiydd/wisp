# Wisp Compliance Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bring the Rust workspace, CLI behavior, documentation, and CI gates into alignment with the project's stated architecture and safety goals.

**Architecture:** Keep the existing five-layer model. Presentation crates may format output and collect user input, but behavior that controls plans, deletion, history, and machine-readable events should live behind `wisp-engine` or lower layers. Prefer small fixes that make current claims true over new feature expansion.

**Tech Stack:** Rust 2024, Cargo workspace resolver 3, clap, tokio, serde_json, thiserror/color-eyre, ratatui, cargo-deny, GitHub Actions.

---

## File Map

- Modify: `.github/workflows/ci.yml` - make CI run the same full workspace checks documented for contributors.
- Modify: `Cargo.toml` - decide whether test `unwrap/expect` are allowed or forbidden, then encode that lint policy explicitly.
- Modify: `README.md` and `README_EN.md` - make user-facing claims match shipped behavior.
- Modify: `CLAUDE.md` and `CONTRIBUTING.md` - align local validation commands and cleaner checklist with real files.
- Modify: `docs/adding-a-cleaner.md` - remove references to missing docs or add the missing cleaner catalog.
- Create: `docs/cleaners.md` - document registered cleaners if the checklist keeps requiring it.
- Modify: `crates/wisp-cli/src/cli.rs` - fix history verb naming and add missing CLI aliases only where compatibility is needed.
- Modify: `crates/wisp-cli/src/main.rs` - fix `--no-trash`, JSON purity, unimplemented command exit codes, and history/config/profile behavior.
- Modify: `crates/wisp-engine/src/lib.rs` - confirm by per-action risk and preserve plan failure visibility.
- Modify: `crates/wisp-engine/src/history.rs` and `crates/wisp-engine/src/audit.rs` - align XDG state path and expose path helpers for tests.
- Test: add/extend tests in `crates/wisp-cli`, `crates/wisp-engine`, and `crates/wisp-core` as described below.

---

### Task 1: Make CI And Local Validation Mean The Same Thing

**Files:**
- Modify: `.github/workflows/ci.yml`
- Modify: `README.md`
- Modify: `README_EN.md`
- Modify: `CLAUDE.md`
- Modify: `CONTRIBUTING.md`
- Modify: `Cargo.toml`

- [ ] **Step 1: Pick the lint policy for tests**

Decision: allow `unwrap/expect` in tests, forbid them in production. This is already how the code is written, and it avoids rewriting 123 low-value test setup calls.

- [ ] **Step 2: Encode the test exception**

In `Cargo.toml`, add this section under the existing lint config:

```toml
[workspace.lints.clippy]
unwrap_used = "warn"
expect_used = "warn"

[workspace.metadata.wisp]
lint_policy = "unwrap_used and expect_used are forbidden in production by clippy -D warnings, but allowed in tests through cfg_attr at crate roots."
```

Then add crate-level cfg attributes to test-heavy crates:

```rust
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
```

Apply this to:
- `crates/wisp-core/src/lib.rs`
- `crates/wisp-cleaners/src/lib.rs`
- `crates/wisp-engine/src/lib.rs`

- [ ] **Step 3: Run full clippy and verify it now passes**

Run:

```sh
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

Expected: exit 0.

- [ ] **Step 4: Update CI to use workspace clippy**

Change `.github/workflows/ci.yml` clippy step from:

```yaml
- run: cargo clippy --all-targets --all-features -- -D warnings
```

to:

```yaml
- run: cargo clippy --workspace --all-targets --all-features -- -D warnings
```

- [ ] **Step 5: Update documented local validation commands**

In `README.md`, `README_EN.md`, `CLAUDE.md`, and `CONTRIBUTING.md`, use:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
cargo deny check
```

- [ ] **Step 6: Verify**

Run:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
cargo deny check
```

Expected:
- fmt: exit 0
- clippy: exit 0
- tests: all current unit tests pass
- deny: `advisories ok, bans ok, licenses ok, sources ok`, duplicate warnings acceptable unless Task 8 chooses to reduce them

- [ ] **Step 7: Commit**

```sh
git add Cargo.toml .github/workflows/ci.yml README.md README_EN.md CLAUDE.md CONTRIBUTING.md crates/wisp-core/src/lib.rs crates/wisp-cleaners/src/lib.rs crates/wisp-engine/src/lib.rs
git commit -m "ci: enforce full workspace validation"
```

---

### Task 2: Fix Machine-Readable Output

**Files:**
- Modify: `crates/wisp-cli/src/main.rs`
- Test: add CLI integration tests if a test harness is introduced, otherwise add focused unit tests around output rendering helpers.

- [ ] **Step 1: Write the failing behavior check**

Run before changing code:

```sh
cargo build --workspace
target/debug/wisp clean @user -n --output json > /tmp/wisp-clean.json
jq type /tmp/wisp-clean.json
```

Expected before fix: `jq` fails because stdout contains JSON plus `[DRY RUN] No changes made.`

- [ ] **Step 2: Route dry-run human text only to human output**

In `dispatch_clean`, change:

```rust
if global.dry_run {
    println!("\n[DRY RUN] No changes made.");
    return Ok(0);
}
```

to:

```rust
if global.dry_run {
    if global.output == cli::OutputFormat::Human {
        println!("\n[DRY RUN] No changes made.");
    }
    return Ok(0);
}
```

- [ ] **Step 3: Keep progress text out of JSON stdout**

Change the plan-building status output so it only runs for human output:

```rust
let show_progress = !global.quiet && global.output == cli::OutputFormat::Human;
if show_progress {
    eprint!("Building plan for '{target}'...");
}
let plan = engine.build_plan(&[target.as_str()]).await?;
if show_progress {
    eprintln!(" done.");
}
```

- [ ] **Step 4: Verify JSON and JSONL**

Run:

```sh
cargo build --workspace
target/debug/wisp clean @user -n --output json > /tmp/wisp-clean.json
jq type /tmp/wisp-clean.json
target/debug/wisp clean @user -n --output jsonl > /tmp/wisp-clean.jsonl
jq -c . /tmp/wisp-clean.jsonl >/dev/null
```

Expected:
- `jq type` prints `"object"`
- JSONL command exits 0

- [ ] **Step 5: Commit**

```sh
git add crates/wisp-cli/src/main.rs
git commit -m "fix(cli): keep machine output parseable"
```

---

### Task 3: Wire `--no-trash` And Resolve `--purge` Documentation

**Files:**
- Modify: `crates/wisp-cli/src/cli.rs`
- Modify: `crates/wisp-cli/src/main.rs`
- Modify: `README.md`
- Modify: `README_EN.md`

- [ ] **Step 1: Choose user-facing spelling**

Decision: keep `--no-trash` as the implemented flag and add `--purge` as an alias for compatibility with README.

- [ ] **Step 2: Add alias in clap**

In `GlobalOpts`, change the flag definition to:

```rust
/// Delete directly without moving to the trash.
#[arg(long, alias = "purge", global = true)]
pub no_trash: bool,
```

- [ ] **Step 3: Apply the flag to engine config**

In `run`, change:

```rust
prefer_trash: cfg.clean.prefer_trash,
```

to:

```rust
prefer_trash: cfg.clean.prefer_trash && !cli.global.no_trash,
```

- [ ] **Step 4: Update README wording**

Use this wording in Chinese README:

```md
删除默认走系统回收站；如需永久删除请加 `--no-trash`（兼容别名 `--purge`）。
```

Use this wording in English README:

```md
Deletions go to trash by default; use `--no-trash` (alias: `--purge`) for permanent removal.
```

- [ ] **Step 5: Verify flag behavior**

Run:

```sh
target/debug/wisp --help | rg -- '--no-trash|--purge'
target/debug/wisp clean @user -n --no-trash --output json | jq '.data.actions[] | select(.via=="trash")' | wc -l
target/debug/wisp clean @user -n --purge --output json | jq '.data.actions[] | select(.via=="trash")' | wc -l
```

Expected:
- help shows `--no-trash`
- both count commands print `0`

- [ ] **Step 6: Commit**

```sh
git add crates/wisp-cli/src/cli.rs crates/wisp-cli/src/main.rs README.md README_EN.md
git commit -m "fix(cli): honor no-trash cleanup mode"
```

---

### Task 4: Make Unimplemented Commands Honest

**Files:**
- Modify: `crates/wisp-cli/src/main.rs`
- Modify: `README.md`
- Modify: `README_EN.md`

- [ ] **Step 1: Add a small helper**

Add near the dispatch helpers:

```rust
fn not_implemented(feature: &str) -> i32 {
    eprintln!("{feature}: not implemented yet.");
    70
}
```

- [ ] **Step 2: Return non-zero for placeholders**

Replace placeholder branches with `return Ok(not_implemented("..."));`.

Examples:

```rust
cli::StateSubcommand::Export { path } => {
    return Ok(not_implemented(&format!("state export {}", path.display())));
}
```

```rust
fn dispatch_profile(_args: cli::ProfileArgs) -> Result<i32> {
    Ok(not_implemented("profile management"))
}
```

- [ ] **Step 3: Fix README command tables to mark incomplete commands**

Either remove incomplete commands from quick-start sections or add a clearly scoped "planned" section. Do not list unimplemented commands as if they work.

- [ ] **Step 4: Verify**

Run:

```sh
target/debug/wisp profile list; echo $?
target/debug/wisp config set clean.default_group @dev; echo $?
target/debug/wisp state export /tmp/wisp-state.tar; echo $?
```

Expected: each command prints a not-implemented message and exits `70`.

- [ ] **Step 5: Commit**

```sh
git add crates/wisp-cli/src/main.rs README.md README_EN.md
git commit -m "fix(cli): report placeholder commands as unavailable"
```

---

### Task 5: Align History Verbs And Scope

**Files:**
- Modify: `crates/wisp-cli/src/cli.rs`
- Modify: `crates/wisp-cli/src/main.rs`
- Modify: `README.md`
- Modify: `README_EN.md`

- [ ] **Step 1: Add `restore` while preserving `undo` as alias**

Change `HistorySubcommand` from `Undo { id: String }` to:

```rust
/// Restore a trashed item from a history entry.
#[command(alias = "undo")]
Restore { id: String },
```

- [ ] **Step 2: Update dispatch branch**

Change:

```rust
cli::HistorySubcommand::Undo { .. } => {
    eprintln!("Undo is only possible for entries deleted via trash (Phase 6).");
}
```

to:

```rust
cli::HistorySubcommand::Restore { id } => {
    eprintln!("history restore {id}: not implemented yet.");
    return Ok(70);
}
```

- [ ] **Step 3: Fix docs**

Use `history list|show|restore|clear`, and state that restore is currently planned unless Task 6 implements it fully.

- [ ] **Step 4: Verify**

Run:

```sh
target/debug/wisp history restore abc; echo $?
target/debug/wisp history undo abc; echo $?
target/debug/wisp history --help | rg 'restore|undo'
```

Expected:
- both restore and undo parse
- both exit `70`
- help shows restore

- [ ] **Step 5: Commit**

```sh
git add crates/wisp-cli/src/cli.rs crates/wisp-cli/src/main.rs README.md README_EN.md
git commit -m "fix(cli): align history restore command"
```

---

### Task 6: Fix Per-Action Risk Confirmation

**Files:**
- Modify: `crates/wisp-engine/src/lib.rs`
- Test: extend tests in `crates/wisp-engine/src/lib.rs`

- [ ] **Step 1: Add a test for mixed risk plans**

Create a test-only confirmer that records requests:

```rust
#[derive(Default)]
struct RecordingConfirmer {
    risks: std::sync::Mutex<Vec<RiskLevel>>,
}
```

Use a two-action plan with risks `[RiskLevel::Safe, RiskLevel::Dangerous]` and `auto_approve_up_to: RiskLevel::Safe`.

Expected failing behavior before fix: confirmer is called with `Dangerous` for both actions.

- [ ] **Step 2: Use per-action risk**

In `Engine::execute`, compute:

```rust
let action_risk = plan.risks.get(idx).copied().unwrap_or(plan.risk);
let needs_confirm = !auto_approve_all && action_risk > self.config.auto_approve_up_to;
```

And pass `risk: action_risk` in `ConfirmRequest`.

- [ ] **Step 3: Verify**

Run:

```sh
cargo test --package wisp-engine --all-targets
cargo test --workspace --all-targets --all-features
```

Expected: new test passes and all existing tests pass.

- [ ] **Step 4: Commit**

```sh
git add crates/wisp-engine/src/lib.rs
git commit -m "fix(engine): confirm cleanup actions by individual risk"
```

---

### Task 7: Make Planning Failures Visible

**Files:**
- Modify: `crates/wisp-core/src/types.rs`
- Modify: `crates/wisp-engine/src/lib.rs`
- Modify: `crates/wisp-cli/src/main.rs`

- [ ] **Step 1: Extend `CleanPlan` with warnings**

Add:

```rust
#[serde(default)]
pub warnings: Vec<String>,
```

to `CleanPlan`.

- [ ] **Step 2: Capture cleaner planning errors**

In `Engine::build_plan`, collect warnings:

```rust
let mut warnings = Vec::new();
```

On cleaner error:

```rust
let msg = format!("{}: {e}", entry.meta.id());
warn!(id = %entry.meta.id(), error = %e, "cleaner plan failed");
warnings.push(msg);
```

Then include `warnings` in the returned `CleanPlan`.

- [ ] **Step 3: Surface warnings in CLI JSON envelope**

When creating `OutputEnvelope`, set `warnings` from the plan warnings if the type supports it. If `OutputEnvelope::new` does not support warnings, add:

```rust
impl<T> OutputEnvelope<T> {
    pub fn with_warnings(mut self, warnings: Vec<String>) -> Self {
        self.warnings = warnings;
        self
    }
}
```

- [ ] **Step 4: Verify**

Run:

```sh
cargo test --package wisp-core --all-targets
cargo test --package wisp-engine --all-targets
cargo test --workspace --all-targets --all-features
```

Expected: serialization tests updated for the new default field, all pass.

- [ ] **Step 5: Commit**

```sh
git add crates/wisp-core/src/types.rs crates/wisp-engine/src/lib.rs crates/wisp-cli/src/main.rs
git commit -m "fix(engine): expose cleaner planning warnings"
```

---

### Task 8: Align XDG State Path And Documentation

**Files:**
- Modify: `crates/wisp-core/src/config.rs`
- Modify: `crates/wisp-engine/src/history.rs`
- Modify: `crates/wisp-engine/src/audit.rs`
- Modify: `README.md`
- Modify: `README_EN.md`
- Modify: `plan.md`

- [ ] **Step 1: Decide path**

Decision: use the actual XDG state location `$XDG_STATE_HOME/wisp`, defaulting to `~/.local/state/wisp`.

- [ ] **Step 2: Implement a shared state dir helper**

In `Config::state_dir`, avoid `data_local_dir().join("state")`. Use `std::env::var_os("XDG_STATE_HOME")` first, then `HOME/.local/state`.

Implementation shape:

```rust
pub fn state_dir() -> Option<PathBuf> {
    if let Some(base) = std::env::var_os("XDG_STATE_HOME") {
        return Some(PathBuf::from(base).join("wisp"));
    }
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(".local/state/wisp"))
}
```

- [ ] **Step 3: Make history/audit call the shared helper**

In `history_path()` and audit path helpers, call `wisp_core::config::Config::state_dir()`.

- [ ] **Step 4: Verify doctor output**

Run:

```sh
target/debug/wisp doctor | rg 'State dir'
```

Expected: path contains `.local/state/wisp`.

- [ ] **Step 5: Verify tests**

Run:

```sh
cargo test --package wisp-core --all-targets
cargo test --package wisp-engine --all-targets
```

Expected: all pass.

- [ ] **Step 6: Commit**

```sh
git add crates/wisp-core/src/config.rs crates/wisp-engine/src/history.rs crates/wisp-engine/src/audit.rs README.md README_EN.md plan.md
git commit -m "fix(state): use xdg state directory consistently"
```

---

### Task 9: Add The Missing Cleaner Catalog

**Files:**
- Create: `docs/cleaners.md`
- Modify: `README.md`
- Modify: `README_EN.md`
- Modify: `docs/adding-a-cleaner.md`

- [ ] **Step 1: Create `docs/cleaners.md`**

Include one row per current cleaner:

```md
# Cleaners

| ID | Group | Risk | Action Type | Description |
| -- | -- | -- | -- | -- |
| arch.journal | System | Safe | external | Vacuum systemd journal. |
| arch.pacman | System | Safe | delete/external | Clean old pacman package cache entries. |
| arch.orphans | System | Moderate | external | Remove orphan packages. |
| system.tmp | System | Dangerous | delete | Clean selected `/tmp` children. |
| user.thumbnails | User | Trivial | delete | Remove thumbnail cache. |
| user.browser_cache | User | Trivial | delete | Remove browser rebuildable caches. |
| user.browser_state | User | Dangerous | delete | Remove browser site/session state, preserving passwords/bookmarks/history. |
| user.trash | User | Safe | delete | Empty user trash files and metadata. |
| user.flatpak | User | Moderate | external | Uninstall unused Flatpak runtimes/extensions. |
| user.linuxqq_cache | User | Safe | delete | Remove LinuxQQ logs and rebuildable caches. |
| user.linuxqq_media | User | Dangerous | delete | Remove LinuxQQ media cache directories. |
| dev.cargo | Dev | Safe | delete | Remove Cargo registry/git caches. |
| dev.npm | Dev | Safe | external | Clean npm cache. |
| dev.pip | Dev | Safe | delete | Remove pip cache. |
| dev.go | Dev | Safe | delete | Remove Go module cache. |
| dev.docker | Dev | Moderate | external | Prune Docker dangling/build-cache data. |
```

- [ ] **Step 2: Link the catalog**

Add links from README files and `docs/adding-a-cleaner.md`.

- [ ] **Step 3: Verify cleaner count**

Run:

```sh
target/debug/wisp clean list | tail -n +3 | wc -l
rg '^\|' docs/cleaners.md | tail -n +3 | wc -l
```

Expected: both counts are `16`.

- [ ] **Step 4: Commit**

```sh
git add docs/cleaners.md README.md README_EN.md docs/adding-a-cleaner.md
git commit -m "docs: document registered cleaners"
```

---

### Task 10: Final Verification And Release Readiness Check

**Files:**
- No code changes unless a verification failure identifies a specific fix.

- [ ] **Step 1: Run the full validation suite**

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
cargo deny check
cargo build --workspace
```

Expected: all commands exit 0. `cargo deny` duplicate warnings are acceptable only if `deny.toml` keeps `multiple-versions = "warn"`.

- [ ] **Step 2: Run key CLI smoke tests**

```sh
target/debug/wisp --help
target/debug/wisp clean list
target/debug/wisp doctor
target/debug/wisp clean @user -n --output json | jq type
target/debug/wisp clean @user -n --output jsonl | jq -c . >/dev/null
target/debug/wisp history restore abc; test $? -eq 70
target/debug/wisp profile list; test $? -eq 70
```

Expected:
- help/list/doctor exit 0
- JSON parses as object
- JSONL parses line by line
- intentionally unavailable commands exit 70

- [ ] **Step 3: Check docs for stale claims**

Run:

```sh
rg -n 'restore|undo|--purge|--no-trash|not yet implemented|Phase 5|Phase 6|local/share/wisp/state|local/state/wisp|cargo clippy --all-targets|cargo test  --all-targets' README.md README_EN.md CLAUDE.md CONTRIBUTING.md docs plan.md
```

Expected:
- no stale `undo` as primary command
- `--purge` only appears as alias
- no user-facing feature is described as implemented when it exits 70
- validation commands include `--workspace`
- state path wording is consistent

- [ ] **Step 4: Commit final doc or verification fixes**

If Step 3 required changes:

```sh
git add README.md README_EN.md CLAUDE.md CONTRIBUTING.md docs plan.md
git commit -m "docs: align shipped behavior with project claims"
```

If no changes were required, do not create an empty commit.

---

## Self-Review

**Spec coverage:** The plan covers all review findings: JSON purity, history mismatch, placeholder commands, `--no-trash`, per-action risk, planning warnings, XDG state path, missing cleaner docs, CI/workspace clippy, and final smoke tests.

**Placeholder scan:** The plan avoids deferred implementation text in execution steps. Commands marked unavailable intentionally return exit 70 until a separate feature plan implements them fully.

**Type consistency:** The plan uses existing names: `GlobalOpts.no_trash`, `EngineConfig.prefer_trash`, `CleanPlan.risks`, `RiskLevel`, `HistorySubcommand`, `Config::state_dir`, and `OutputEnvelope`.

---

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-06-05-wisp-compliance-fixes.md`.

Two execution options:

**1. Subagent-Driven (recommended)** - dispatch a fresh subagent per task, review between tasks, fast iteration.

**2. Inline Execution** - execute tasks in this session using executing-plans, with checkpoints after each task group.
