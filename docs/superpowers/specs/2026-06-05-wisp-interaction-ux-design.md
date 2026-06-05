# Wisp Interaction UX Design

Date: 2026-06-05
Branch: codex/cli-ux-simplify

## Goal

Make wisp easier to operate without reducing safety or script compatibility. The default path should answer three questions quickly:

- What will be cleaned?
- How much space can be freed?
- What should I do next?

This design covers three approved directions: human `clean` preview output, cleaner discovery commands, and TUI interaction alignment.

## Non-goals

- Do not change JSON or JSONL schemas.
- Do not make `clean` execute by default.
- Do not delete or hide working advanced commands beyond the current help-hiding policy.
- Do not add new cleaners in this pass.

## Direction 1: Clean Preview Output

`wisp clean` should stay the primary command. Its human output should become a compact preview instead of a raw action dump.

The preview will show a summary first: target label, mode, action count, estimated size, highest risk, and whether high-risk actions are excluded. It will then group actions by type:

- Files/directories to delete
- External commands to run

The default human preview should show only the first several actions per group and report how many are hidden. This keeps common output to one readable screen while still showing enough detail to build trust. Users who need complete machine-readable detail can keep using `--output json`.

The current progress message must stop interleaving with the plan body. Plan-building status should go to stderr and finish before stdout rendering starts.

Final dry-run guidance should be explicit:

- `Run: wisp clean --apply`
- `Inspect: wisp clean list`
- `Include high risk: wisp clean --deep`

## Direction 2: Cleaner Discovery

`wisp clean list` and `wisp clean info <target>` are advanced commands, but they should be useful when users reach for them.

`clean list` should present concise columns with lowercase, stable values: ID, group, risk, root, and name. It should print a short footer showing how to filter by group/risk and how to inspect a single cleaner.

`clean info <target>` should present a readable detail block and include the exact command to preview that cleaner:

`wisp clean <id>`

Ambiguous targets should continue to fail safely, but the error should show matching IDs and tell users to use the full ID.

## Direction 3: TUI Alignment

The TUI should match the simplified CLI mental model. The home page should lead with a recommended clean path and keep specific groups as secondary choices.

The home menu should be reorganized around these entries:

- Analyze disk usage
- Recommended clean
- Clean by area
- History
- Quit

`Recommended clean` maps to the same concept as CLI `wisp clean`: user + dev cleanup, excluding dangerous actions by default. `Clean by area` keeps user/system/dev/LinuxQQ entries available without making them the first thing users must understand.

The cleaner page should make mode and risk clearer before execution. In idle state, it should show:

- Selected scope
- What the scope includes
- Safety note for dangerous actions
- Primary actions: preview, apply, back

This pass does not need a full TUI execution rewrite. It should prioritize labels, grouping, and explanatory hints that reduce decision load.

## Architecture

Keep the current separation:

- CLI parsing remains in `crates/wisp-cli/src/cli.rs`.
- CLI rendering and dispatch remain in `crates/wisp-cli/src/main.rs`.
- TUI menu and cleaner layout remain in `crates/wisp-tui/src/pages/home.rs` and `crates/wisp-tui/src/pages/cleaner.rs`.

Add small formatting helpers in `main.rs` only if they keep output code readable. Avoid introducing a generic rendering framework.

## Data Flow

CLI clean flow remains:

1. Parse `CleanArgs`.
2. Build the engine plan.
3. For default recommended clean, filter dangerous actions unless `--deep`.
4. Render human/json/jsonl output.
5. Execute only when `--apply` is set.

TUI flow remains page-driven. Home page selection routes into existing page actions; cleaner page uses existing group state.

## Error Handling

- Unknown cleaner: keep exit code `1`, but use actionable text.
- Ambiguous cleaner: keep exit code `64`, list matching IDs.
- Empty plans: keep exit code `0`, but include next-step guidance.
- JSON/JSONL output: do not add human progress or hints.

## Testing

Add or update CLI tests for:

- Human clean preview includes summary and next-step guidance.
- Root help stays concise.
- Clean list help/discovery remains available.
- JSON clean output remains parseable and unchanged in shape.

For TUI, prefer unit-level assertions around menu item ordering or labels if existing code structure allows it. If that would require large refactoring, keep the TUI change small and verify with compile/clippy plus focused manual source review.

## Acceptance Criteria

- `wisp clean` human output is compact, grouped, and not interleaved with progress text.
- `wisp clean --output json` remains valid JSON with the existing envelope structure.
- `wisp clean list` is easier to scan and includes discovery hints.
- TUI home page presents recommended clean before specialized clean groups.
- All changes pass `cargo fmt`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo test --workspace --all-targets --all-features`, and `cargo build --workspace`.
