# Adding a Cleaner

Follow this checklist when adding a new cleaner to `wisp-cleaners`.

## Step-by-step

### 1. Create the module

```
crates/wisp-cleaners/src/system/my_cleaner.rs   # or user/ or dev/
```

### 2. Implement `CleanerMeta`

```rust
use wisp_core::types::{CleanAction, CleanerId, CleanerGroup, CleanerMeta, RiskLevel};
use wisp_platform::{Distro, DistroKind};

struct MyCleaner;

impl CleanerMeta for MyCleaner {
    fn id(&self) -> CleanerId { CleanerId::new("arch.my_cleaner") }
    fn name(&self) -> &str { "My Cleaner" }
    fn description(&self) -> &str { "Removes ..." }
    fn risk(&self) -> RiskLevel { RiskLevel::Safe }
    fn requires_root(&self) -> bool { false }
    fn supported_on(&self, distro: &dyn Distro) -> bool {
        distro.kind() == DistroKind::Arch
    }
    fn group(&self) -> CleanerGroup { CleanerGroup::System }
}
```

### 3. Implement `CleanerExec`

```rust
use crate::{CleanCtx, CleanerExec};

impl CleanerExec for MyCleaner {
    async fn plan(&self, ctx: &CleanCtx) -> wisp_core::CoreResult<Vec<CleanAction>> {
        if ctx.dry_run {
            // Collect paths but return them without actually touching anything.
        }
        Ok(vec![
            CleanAction::Delete {
                path: "/path/to/file".try_into()?,
                size: 0,
                via: wisp_core::types::DeletionVia::Trash,
            },
        ])
    }
}
```

### 4. Register via linkme

At the bottom of the module file:

```rust
static MY_CLEANER_IMPL: MyCleaner = MyCleaner;

#[linkme::distributed_slice(crate::CLEANERS)]
static MY_CLEANER_ENTRY: crate::CleanerEntry = crate::CleanerEntry {
    meta: &MY_CLEANER_IMPL,
};
```

### 5. Expose the module

In `crates/wisp-cleaners/src/system/mod.rs` (or `user/` / `dev/`):

```rust
pub mod my_cleaner;
```

### 6. Tests

```rust
#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn no_path_traversal(s in ".*\\.\\..*") {
            // assert the cleaner rejects paths with `..`
        }
    }
}
```

### 7. Update docs

Add an entry to `docs/cleaners.md` with: id, name, risk, description, what it
deletes, whether it requires root.
