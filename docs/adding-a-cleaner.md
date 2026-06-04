# Adding a Cleaner

Follow this checklist when adding a new cleaner to `wisp-cleaners`. Keep
[docs/cleaners.md](cleaners.md) in sync with every registered cleaner.

## Step-by-step

### 1. Create the module

```
crates/wisp-cleaners/src/system/my_cleaner.rs   # or user/ or dev/
```

### 2. Implement `CleanerMeta`

```rust
use wisp_core::types::{CleanerGroup, CleanerId, CleanerMeta, RiskLevel};
use wisp_platform::{Distro, DistroKind};

use crate::{CLEANERS, CleanCtx, CleanerEntry, PlanFuture, delete_home_subdirs};

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

### 3. Implement the plan function

```rust
fn plan<'a>(_ctx: &'a CleanCtx) -> PlanFuture<'a> {
    Box::pin(async move {
        Ok(delete_home_subdirs(
            &[".cache/my-tool"],
            wisp_core::types::DeletionVia::Direct,
        ))
    })
}
```

The plan function must only describe actions. Deletion and external-command
execution happen later in `wisp-engine`, where dry-run, confirmation, path
safety, history, and audit logging are enforced.

### 4. Register via linkme

At the bottom of the module file:

```rust
static META: MyCleaner = MyCleaner;

#[linkme::distributed_slice(CLEANERS)]
static ENTRY: CleanerEntry = CleanerEntry { meta: &META, plan };
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

Add an entry to [docs/cleaners.md](cleaners.md) with: id, group, risk, action
type, root requirement, and a short description.
