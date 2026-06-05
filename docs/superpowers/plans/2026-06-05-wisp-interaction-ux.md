# Wisp Interaction UX Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Improve wisp's day-to-day interaction by making clean previews compact, cleaner discovery easier to scan, and the TUI match the simplified CLI workflow.

**Architecture:** Keep clap parsing and command dispatch in `wisp-cli`, with human rendering helpers returning strings for testability. Keep TUI changes focused on labels, menu order, and idle-page guidance in the existing page modules. JSON/JSONL schemas and execution behavior remain unchanged.

**Tech Stack:** Rust 2024, clap, tokio, serde_json, ratatui, cargo fmt/clippy/test.

---

## File Map

- Modify `crates/wisp-cli/src/main.rs`: clean human preview rendering, cleaner list/info output, and unit tests for pure formatting helpers.
- Modify `crates/wisp-tui/src/pages/home.rs`: make recommended clean the first cleaning path and keep specific scopes secondary.
- Modify `crates/wisp-tui/src/pages/cleaner.rs`: clarify idle-state scope/safety/action labels.
- Modify `README.md` and `README_EN.md`: update examples only if CLI/TUI wording changes materially.

## Task 1: Compact Human Clean Preview

**Files:**
- Modify: `crates/wisp-cli/src/main.rs`

- [ ] **Step 1: Add failing tests for human preview structure**

Add a `#[cfg(test)] mod tests` to `crates/wisp-cli/src/main.rs` with a small `CleanPlan` fixture and assertions:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use camino::Utf8PathBuf;
    use uuid::Uuid;
    use wisp_engine::types::{
        CleanAction, CleanPlan, DeletionVia, PrivilegeRequirement, RiskLevel,
    };

    fn sample_plan() -> CleanPlan {
        CleanPlan {
            id: Uuid::nil(),
            actions: vec![
                CleanAction::Delete {
                    path: Utf8PathBuf::from("/tmp/a"),
                    size: 1024,
                    via: DeletionVia::Direct,
                },
                CleanAction::RunExternal {
                    cmd: wisp_engine::types::ExternalCommand {
                        program: "npm".into(),
                        args: vec!["cache".into(), "clean".into(), "--force".into()],
                    },
                    estimated_size: None,
                },
            ],
            risks: vec![RiskLevel::Safe, RiskLevel::Moderate],
            estimated_size: 1024,
            required_privileges: PrivilegeRequirement {
                requires_root: false,
            },
            risk: RiskLevel::Moderate,
            warnings: Vec::new(),
        }
    }

    #[test]
    fn human_preview_starts_with_summary_and_next_steps() {
        let rendered = format_plan_human(
            &sample_plan(),
            CleanDisplayOptions {
                target_label: "recommended",
                dry_run: true,
                recommended: true,
                deep: false,
            },
        );

        assert!(rendered.contains("Preview: recommended"));
        assert!(rendered.contains("Estimated reclaim:"));
        assert!(rendered.contains("Files and directories"));
        assert!(rendered.contains("External commands"));
        assert!(rendered.contains("Run: wisp clean --apply"));
        assert!(rendered.contains("Include high risk: wisp clean --deep"));
    }
}
```

- [ ] **Step 2: Run the test and verify it fails**

Run:

```bash
cargo test --package wisp-cli human_preview_starts_with_summary_and_next_steps --all-targets
```

Expected: fails because `format_plan_human` and `CleanDisplayOptions` do not exist.

- [ ] **Step 3: Implement compact preview rendering**

In `crates/wisp-cli/src/main.rs`, replace `print_plan_human` with a pure formatter and a small printing wrapper:

```rust
const HUMAN_PREVIEW_LIMIT: usize = 8;

#[derive(Clone, Copy)]
struct CleanDisplayOptions<'a> {
    target_label: &'a str,
    dry_run: bool,
    recommended: bool,
    deep: bool,
}

fn print_plan_human(plan: &wisp_engine::types::CleanPlan, options: CleanDisplayOptions<'_>) {
    print!("{}", format_plan_human(plan, options));
}

fn format_plan_human(
    plan: &wisp_engine::types::CleanPlan,
    options: CleanDisplayOptions<'_>,
) -> String {
    let title = if options.dry_run { "Preview" } else { "Plan" };
    let mut out = String::new();
    out.push_str(&format!("{title}: {}\n", options.target_label));
    out.push_str(&format!(
        "Mode: {}  Actions: {}  Risk: {}  Estimated reclaim: {}\n",
        if options.dry_run { "preview" } else { "apply" },
        plan.actions.len(),
        risk_label(plan.risk),
        humansize::format_size(plan.estimated_size, humansize::DECIMAL)
    ));
    if options.recommended && !options.deep {
        out.push_str("High-risk actions are excluded. Use `wisp clean --deep` to preview them.\n");
    }
    out.push('\n');
    out.push_str("Files and directories\n");
    out.push_str("---------------------\n");
    // Append up to HUMAN_PREVIEW_LIMIT delete actions, then a hidden-count line.
    out.push_str("\nExternal commands\n");
    out.push_str("-----------------\n");
    // Append up to HUMAN_PREVIEW_LIMIT external commands, then a hidden-count line.
    if options.dry_run {
        out.push_str("\nNext steps\n");
        out.push_str("  Run: wisp clean --apply\n");
        out.push_str("  Inspect: wisp clean list\n");
        out.push_str("  Include high risk: wisp clean --deep\n");
    }
    out
}
```

Use existing `humansize::format_size` and `wisp_engine::types::CleanAction`. Keep JSON/JSONL paths unchanged.

- [ ] **Step 4: Pass display options through dispatch**

In `dispatch_clean`, compute:

```rust
let recommended = args.target.is_none();
let display_options = CleanDisplayOptions {
    target_label: &target_label,
    dry_run,
    recommended,
    deep: args.deep,
};
```

Then call:

```rust
cli::OutputFormat::Human => print_plan_human(&plan, display_options),
```

Remove the separate dry-run footer because the formatter now includes it.

- [ ] **Step 5: Fix progress interleaving**

Change the plan-building status from inline stderr to full stderr lines:

```rust
if show_progress {
    eprintln!("Building plan for '{target_label}'...");
}
// build plan
if show_progress {
    eprintln!("Plan ready.");
}
```

Expected: stdout plan body no longer has `done.` inserted into the action list.

- [ ] **Step 6: Verify and commit**

Run:

```bash
cargo fmt --all
cargo test --package wisp-cli human_preview_starts_with_summary_and_next_steps --all-targets
target/debug/wisp clean
target/debug/wisp clean --output json | python3 -m json.tool
```

Expected: test passes, human output is compact, JSON parses.

Commit:

```bash
git add crates/wisp-cli/src/main.rs
git commit -m "codex fix: compact clean preview output"
```

## Task 2: Cleaner Discovery Output

**Files:**
- Modify: `crates/wisp-cli/src/main.rs`

- [ ] **Step 1: Add tests for discovery formatting**

Add tests in the existing `main.rs` test module:

```rust
#[test]
fn cleaner_list_footer_explains_filters_and_info() {
    let rendered = format_cleaner_list(None, None);
    assert!(rendered.contains("wisp clean list --group dev"));
    assert!(rendered.contains("wisp clean info <id>"));
    assert!(rendered.contains("ROOT"));
}

#[test]
fn cleaner_info_includes_preview_command() {
    let rendered = format_cleaner_info("dev.npm").expect("dev.npm cleaner exists");
    assert!(rendered.contains("Preview"));
    assert!(rendered.contains("wisp clean dev.npm"));
}
```

- [ ] **Step 2: Run tests and verify failure**

Run:

```bash
cargo test --package wisp-cli cleaner_list_footer_explains_filters_and_info --all-targets
cargo test --package wisp-cli cleaner_info_includes_preview_command --all-targets
```

Expected: fails because formatter helpers do not exist.

- [ ] **Step 3: Refactor cleaner list into a formatter**

Replace direct printing in `print_cleaner_list` with:

```rust
fn print_cleaner_list(group: Option<&str>, risk: Option<&str>) {
    print!("{}", format_cleaner_list(group, risk));
}

fn format_cleaner_list(group: Option<&str>, risk: Option<&str>) -> String {
    let mut out = String::new();
    out.push_str(&format!("{:<22}  {:<8}  {:<9}  {:<4}  NAME\n", "ID", "GROUP", "RISK", "ROOT"));
    out.push_str(&format!("{}\n", "-".repeat(82)));
    for entry in wisp_engine::all_cleaners() {
        let meta = entry.meta;
        let group_text = group_label(meta.group());
        let risk_text = risk_label(meta.risk());
        if let Some(filter) = group && !group_text.contains(filter) {
            continue;
        }
        if let Some(filter) = risk && !risk_text.contains(filter) {
            continue;
        }
        out.push_str(&format!(
            "{:<22}  {:<8}  {:<9}  {:<4}  {}\n",
            meta.id(),
            group_text,
            risk_text,
            bool_label(meta.requires_root()),
            meta.name()
        ));
    }
    out.push_str("\nFilters: wisp clean list --group dev --risk safe\n");
    out.push_str("Inspect: wisp clean info <id>\n");
    out
}
```

Add small helpers:

```rust
fn group_label(group: wisp_engine::types::CleanerGroup) -> &'static str {
    match group {
        wisp_engine::types::CleanerGroup::System => "system",
        wisp_engine::types::CleanerGroup::User => "user",
        wisp_engine::types::CleanerGroup::Dev => "dev",
    }
}

fn risk_label(risk: wisp_engine::types::RiskLevel) -> &'static str {
    match risk {
        wisp_engine::types::RiskLevel::Trivial => "trivial",
        wisp_engine::types::RiskLevel::Safe => "safe",
        wisp_engine::types::RiskLevel::Moderate => "moderate",
        wisp_engine::types::RiskLevel::Dangerous => "dangerous",
    }
}

fn bool_label(value: bool) -> &'static str { if value { "yes" } else { "no" } }
```

- [ ] **Step 4: Refactor cleaner info into a formatter**

Use:

```rust
fn print_cleaner_info(target: &str) -> i32 {
    match format_cleaner_info(target) {
        Ok(rendered) => {
            print!("{rendered}");
            0
        }
        Err(CleanerInfoError::NotFound) => {
            eprintln!("Cleaner '{target}' not found.");
            eprintln!("List cleaners: wisp clean list");
            1
        }
        Err(CleanerInfoError::Ambiguous(matches)) => {
            eprintln!("Target '{target}' matched multiple cleaners:");
            for id in matches {
                eprintln!("  {id}");
            }
            eprintln!("Use a full cleaner ID. List cleaners: wisp clean list");
            64
        }
    }
}
```

The success output includes ID, name, group, risk, root, description, and:

```text
Preview     wisp clean <id>
Apply       wisp clean <id> --apply
```

- [ ] **Step 5: Verify and commit**

Run:

```bash
cargo fmt --all
cargo test --package wisp-cli cleaner_list_footer_explains_filters_and_info cleaner_info_includes_preview_command --all-targets
target/debug/wisp clean list
target/debug/wisp clean info dev.npm
```

Expected: tests pass; list is scan-friendly; info has preview/apply commands.

Commit:

```bash
git add crates/wisp-cli/src/main.rs
git commit -m "codex fix: improve cleaner discovery output"
```

## Task 3: TUI Home and Cleaner Page Alignment

**Files:**
- Modify: `crates/wisp-tui/src/pages/home.rs`
- Modify: `crates/wisp-tui/src/pages/cleaner.rs`

- [ ] **Step 1: Add or expose menu-label tests**

If `MENU_ITEMS` can stay private, add tests inside `home.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recommended_clean_is_first_cleaning_path() {
        assert_eq!(MENU_ITEMS[0].label, "Analyze");
        assert_eq!(MENU_ITEMS[1].label, "Recommended Clean");
    }
}
```

- [ ] **Step 2: Run the test and verify failure**

Run:

```bash
cargo test --package wisp-tui recommended_clean_is_first_cleaning_path --all-targets
```

Expected: fails because the current label is `Quick Clean (User)`.

- [ ] **Step 3: Update home menu labels and actions**

Change `MENU_ITEMS` order to:

```rust
const MENU_ITEMS: &[MenuItem] = &[
    MenuItem {
        label: "Analyze",
        desc: "Find what is using disk space",
        action: MenuAction::Analyze,
    },
    MenuItem {
        label: "Recommended Clean",
        desc: "user + dev caches, dangerous items excluded",
        action: MenuAction::Clean(CleanGroup::All),
    },
    MenuItem {
        label: "Clean User Area",
        desc: "trash · browsers · thumbnails · desktop caches",
        action: MenuAction::Clean(CleanGroup::User),
    },
    MenuItem {
        label: "Clean Dev Caches",
        desc: "cargo · npm · pip · go · docker",
        action: MenuAction::Clean(CleanGroup::Dev),
    },
    MenuItem {
        label: "Clean System Area",
        desc: "pacman cache · journal · orphans",
        action: MenuAction::Clean(CleanGroup::System),
    },
    MenuItem {
        label: "Clean LinuxQQ",
        desc: "QQ caches and optional media cleanup",
        action: MenuAction::Clean(CleanGroup::LinuxQq),
    },
    MenuItem {
        label: "History",
        desc: "View past clean sessions",
        action: MenuAction::History,
    },
    MenuItem {
        label: "Quit",
        desc: "Exit wisp",
        action: MenuAction::Quit,
    },
];
```

- [ ] **Step 4: Update cleaner idle copy**

In `cleaner.rs`, update group descriptions so the idle page makes scope and safety clearer. For `CleanGroup::All`, show:

```rust
CleanGroup::All => &[
    ("Recommended clean", "user + dev rebuildable caches"),
    ("Excluded by default", "dangerous media/history-sensitive actions"),
    ("Use CLI for deep mode", "wisp clean --deep"),
],
```

Rename idle action labels if needed so they read as primary actions: Preview, Apply, Back.

- [ ] **Step 5: Verify and commit**

Run:

```bash
cargo fmt --all
cargo test --package wisp-tui recommended_clean_is_first_cleaning_path --all-targets
cargo build --workspace
```

Expected: test passes and workspace builds.

Commit:

```bash
git add crates/wisp-tui/src/pages/home.rs crates/wisp-tui/src/pages/cleaner.rs
git commit -m "codex fix: align tui with simplified clean flow"
```

## Final Verification

Run on the latest branch head:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
cargo build --workspace
target/debug/wisp --help
target/debug/wisp clean
target/debug/wisp clean list
target/debug/wisp clean info dev.npm
target/debug/wisp clean --output json | python3 -m json.tool
git status --short
```

Expected:

- Formatting, clippy, tests, and build pass.
- Root help stays concise.
- Human clean preview is compact and has next steps.
- Cleaner discovery output is scan-friendly.
- JSON clean output remains parseable.
- Worktree is clean except intentionally ignored/untracked local files outside the branch.

## Self-Review

**Spec coverage:** Task 1 covers compact clean preview and progress interleaving. Task 2 covers cleaner list/info discovery. Task 3 covers TUI home and cleaner-page alignment.

**Placeholder scan:** No task contains open-ended implementation placeholders; every task names exact files, tests, commands, and commit messages.

**Type consistency:** Helper names are consistent across tests and implementation steps: `CleanDisplayOptions`, `format_plan_human`, `format_cleaner_list`, and `format_cleaner_info`.
