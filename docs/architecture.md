# wisp Architecture

## Layer overview

```
┌─────────────────────────────────────────────────┐
│  L5 · Presentation                              │
│  wisp-cli (CLI + Prompt)  │  wisp-tui (TUI)     │
├─────────────────────────────────────────────────┤
│  L4 · Engine (wisp-engine)                      │
│  Plan assembly · Execution · Progress events    │
├─────────────────────────────────────────────────┤
│  L3 · Cleaners (wisp-cleaners)                  │
│  system/ · user/ · dev/                         │
├─────────────────────────────────────────────────┤
│  L2 · FS Core (wisp-core)                       │
│  Directory scan · Path safety · Types           │
├─────────────────────────────────────────────────┤
│  L1 · Platform (wisp-platform)                  │
│  Distro detection · PackageManager · InitSystem │
└─────────────────────────────────────────────────┘
```

Dependencies flow strictly upward (L1 ← L2 ← L3 ← L4 ← L5).
No layer may import from a layer above it.

## Execution model

The Engine exposes an async API backed by `tokio`.  Filesystem scanning
uses `jwalk` (rayon-based) wrapped in `tokio::task::spawn_blocking` so
that L5 only ever sees `async` Futures.

## Cleaner registration

Cleaners self-register via `linkme::distributed_slice`.  Adding a new
cleaner requires zero changes to any registry table.

## Key data flows

```
L5 (CLI/TUI)
  │  wisp clean @user -n
  ▼
L4 Engine::build_plan(targets)
  │  calls each CleanerExec::plan()  (L3)
  │  validates paths                 (L2)
  ▼
CleanPlan
  │
L5 Confirmer::ask()   ← confirmation loop
  │
L4 Engine::execute(plan, tx)
  │  streams ProgressEvent to tx
  │  L2 fs operations (or dry-run fence)
  ▼
CleanReport
```
