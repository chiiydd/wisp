# Common Cleaners Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add conservative cleaners for common user caches, JavaScript toolchain caches, and Python tooling caches without touching user documents, configuration, credentials, browser history, downloads, or project source trees.

**Architecture:** Follow the existing `wisp-cleaners` pattern: each cleaner is a focused module exposing `CleanerMeta`, a `plan` function returning `CleanAction::Delete`, and a `linkme` registry entry. Reuse `delete_home_subdirs` for fixed `$HOME`-relative cache directories and keep all new actions direct deletion because the chosen paths are rebuildable caches. Update docs so `clean list`, docs, and README claims stay aligned.

**Tech Stack:** Rust workspace, `wisp-cleaners`, `wisp-engine`, `linkme`, `tokio` tests, `cargo fmt`, `cargo clippy`, `cargo test`.

---

### Task 1: Add `user.common_cache`

**Files:**
- Create: `crates/wisp-cleaners/src/user/common_cache.rs`
- Modify: `crates/wisp-cleaners/src/user/mod.rs`

- [ ] **Step 1: Write failing tests**

Add tests in `common_cache.rs` proving that present common cache paths produce direct-delete actions and absent paths are skipped:

```rust
#[test]
fn common_cache_collects_only_present_rebuildable_paths() {
    let tmp = tempfile::TempDir::new().unwrap();
    let home = tmp.path();
    std::fs::create_dir_all(home.join(".cache/fontconfig")).unwrap();
    std::fs::create_dir_all(home.join(".cache/mesa_shader_cache")).unwrap();

    let actions = collect_common_cache_actions(home);
    let paths = collect_paths(&actions);

    assert!(paths.iter().any(|p| p.ends_with(".cache/fontconfig")));
    assert!(paths.iter().any(|p| p.ends_with(".cache/mesa_shader_cache")));
    assert!(!paths.iter().any(|p| p.ends_with(".cache/documents")));
    for action in &actions {
        let CleanAction::Delete { via, .. } = action else {
            panic!("common cache cleaner must only emit Delete actions");
        };
        assert_eq!(*via, DeletionVia::Direct);
    }
}
```

- [ ] **Step 2: Verify RED**

Run:

```sh
cargo test --package wisp-cleaners user::common_cache --all-targets
```

Expected: fails because `user::common_cache` does not exist.

- [ ] **Step 3: Implement minimal cleaner**

Create `common_cache.rs` with:
- ID `user.common_cache`
- Name `Common user caches`
- Group `User`
- Risk `Trivial`
- Paths: `.cache/fontconfig`, `.cache/mesa_shader_cache`, `.cache/mesa_shader_cache_db`, `.cache/nvidia`, `.cache/GLCache`, `.cache/gstreamer-1.0`

Register it in `crates/wisp-cleaners/src/user/mod.rs` with `pub mod common_cache;`.

- [ ] **Step 4: Verify GREEN**

Run:

```sh
cargo test --package wisp-cleaners user::common_cache --all-targets
```

Expected: tests pass.

- [ ] **Step 5: Commit**

```sh
git add crates/wisp-cleaners/src/user/common_cache.rs crates/wisp-cleaners/src/user/mod.rs
git commit -m "codex fix: add common user cache cleaner"
```

---

### Task 2: Add `dev.javascript`

**Files:**
- Create: `crates/wisp-cleaners/src/dev/javascript.rs`
- Modify: `crates/wisp-cleaners/src/dev/mod.rs`

- [ ] **Step 1: Write failing tests**

Add tests proving present JavaScript toolchain cache paths produce direct-delete actions:

```rust
#[test]
fn javascript_collects_common_toolchain_caches() {
    let tmp = tempfile::TempDir::new().unwrap();
    let home = tmp.path();
    std::fs::create_dir_all(home.join(".cache/yarn")).unwrap();
    std::fs::create_dir_all(home.join(".local/share/pnpm/store")).unwrap();
    std::fs::create_dir_all(home.join(".bun/install/cache")).unwrap();

    let actions = collect_javascript_cache_actions(home);
    let paths = collect_paths(&actions);

    assert!(paths.iter().any(|p| p.ends_with(".cache/yarn")));
    assert!(paths.iter().any(|p| p.ends_with(".local/share/pnpm/store")));
    assert!(paths.iter().any(|p| p.ends_with(".bun/install/cache")));
    for action in &actions {
        let CleanAction::Delete { via, .. } = action else {
            panic!("javascript cleaner must only emit Delete actions");
        };
        assert_eq!(*via, DeletionVia::Direct);
    }
}
```

- [ ] **Step 2: Verify RED**

Run:

```sh
cargo test --package wisp-cleaners dev::javascript --all-targets
```

Expected: fails because `dev::javascript` does not exist.

- [ ] **Step 3: Implement minimal cleaner**

Create `javascript.rs` with:
- ID `dev.javascript`
- Name `JavaScript toolchain caches`
- Group `Dev`
- Risk `Safe`
- Paths: `.cache/yarn`, `.cache/pnpm`, `.local/share/pnpm/store`, `.npm/_cacache`, `.cache/node-gyp`, `.cache/electron`, `.cache/electron-builder`, `.cache/Cypress`, `.cache/ms-playwright`, `.bun/install/cache`

Register it in `crates/wisp-cleaners/src/dev/mod.rs` with `pub mod javascript;`.

- [ ] **Step 4: Verify GREEN**

Run:

```sh
cargo test --package wisp-cleaners dev::javascript --all-targets
```

Expected: tests pass.

- [ ] **Step 5: Commit**

```sh
git add crates/wisp-cleaners/src/dev/javascript.rs crates/wisp-cleaners/src/dev/mod.rs
git commit -m "codex fix: add javascript cache cleaner"
```

---

### Task 3: Add `dev.python_extra`

**Files:**
- Create: `crates/wisp-cleaners/src/dev/python_extra.rs`
- Modify: `crates/wisp-cleaners/src/dev/mod.rs`

- [ ] **Step 1: Write failing tests**

Add tests proving Python tooling cache paths are collected and deleted directly:

```rust
#[test]
fn python_extra_collects_common_tool_caches() {
    let tmp = tempfile::TempDir::new().unwrap();
    let home = tmp.path();
    std::fs::create_dir_all(home.join(".cache/uv")).unwrap();
    std::fs::create_dir_all(home.join(".cache/pypoetry")).unwrap();
    std::fs::create_dir_all(home.join(".cache/ruff")).unwrap();

    let actions = collect_python_extra_cache_actions(home);
    let paths = collect_paths(&actions);

    assert!(paths.iter().any(|p| p.ends_with(".cache/uv")));
    assert!(paths.iter().any(|p| p.ends_with(".cache/pypoetry")));
    assert!(paths.iter().any(|p| p.ends_with(".cache/ruff")));
    for action in &actions {
        let CleanAction::Delete { via, .. } = action else {
            panic!("python extra cleaner must only emit Delete actions");
        };
        assert_eq!(*via, DeletionVia::Direct);
    }
}
```

- [ ] **Step 2: Verify RED**

Run:

```sh
cargo test --package wisp-cleaners dev::python_extra --all-targets
```

Expected: fails because `dev::python_extra` does not exist.

- [ ] **Step 3: Implement minimal cleaner**

Create `python_extra.rs` with:
- ID `dev.python_extra`
- Name `Python tool caches`
- Group `Dev`
- Risk `Safe`
- Paths: `.cache/uv`, `.cache/pypoetry`, `.cache/pipx`, `.cache/ruff`, `.cache/mypy`, `.cache/pytest`

Register it in `crates/wisp-cleaners/src/dev/mod.rs` with `pub mod python_extra;`.

- [ ] **Step 4: Verify GREEN**

Run:

```sh
cargo test --package wisp-cleaners dev::python_extra --all-targets
```

Expected: tests pass.

- [ ] **Step 5: Commit**

```sh
git add crates/wisp-cleaners/src/dev/python_extra.rs crates/wisp-cleaners/src/dev/mod.rs
git commit -m "codex fix: add python tool cache cleaner"
```

---

### Task 4: Update docs and final verification

**Files:**
- Modify: `docs/cleaners.md`
- Modify: `README.md`
- Modify: `README_EN.md`

- [ ] **Step 1: Update docs**

Add these rows to `docs/cleaners.md`:

```md
| user.common_cache | User | Trivial | delete | no | Remove common rebuildable desktop caches. |
| dev.javascript | Dev | Safe | delete | no | Remove JavaScript toolchain package and build caches. |
| dev.python_extra | Dev | Safe | delete | no | Remove Python tool caches beyond pip. |
```

Update README feature bullets to mention common desktop caches, JavaScript toolchain caches, and Python tool caches.

- [ ] **Step 2: Verify docs and runtime count**

Run:

```sh
cargo fmt --all --check
cargo test --package wisp-cleaners --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
cargo build --workspace
target/debug/wisp clean list | tail -n +3 | wc -l
rg '^\|' docs/cleaners.md | tail -n +3 | wc -l
```

Expected:
- all Cargo commands exit 0
- both counts are `19`

- [ ] **Step 3: Commit**

```sh
git add docs/cleaners.md README.md README_EN.md
git commit -m "codex fix: document common cache cleaners"
```

---

## Self-Review

**Spec coverage:** The plan covers the confirmed conservative scope: common user caches, JavaScript caches, Python tool caches, docs, registration, and verification.

**Placeholder scan:** No implementation step uses open-ended placeholders; each task lists exact paths, cleaner IDs, risks, and verification commands.

**Type consistency:** New cleaners follow existing `CleanerMeta`, `PlanFuture`, `CleanAction::Delete`, `DeletionVia::Direct`, and `linkme::distributed_slice(CLEANERS)` patterns.
