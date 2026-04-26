# Packaging

This directory holds out-of-tree packaging artefacts.

## Layout

```
packaging/
├── README.md            # this file
└── aur/
    ├── README.md        # AUR push workflow
    ├── wisp/PKGBUILD    # stable, sourced from GitHub release tarball
    └── wisp-git/PKGBUILD # VCS, builds from master
```

## GitHub Release

`v*.*.*` tags trigger [`.github/workflows/release.yml`](../.github/workflows/release.yml), which builds binaries for:

- `x86_64-unknown-linux-gnu`
- `x86_64-unknown-linux-musl`
- `aarch64-unknown-linux-gnu`

Each is uploaded as `wisp-<version>-<target>.tar.gz` (with a `.sha256` sidecar) and contains the binary, README, CONTRIBUTING, generated man page, and shell completions.

## crates.io

Three crates are published, in this order (each dependent waits for the previous to finish indexing):

1. `wisp-platform` (L1)
2. `wisp-core` (L2, depends on `wisp-platform`)
3. `wisp-cleaners` (L3, depends on both above)

`wisp-engine`, `wisp-tui`, `wisp-cli` are **not** published — they're tightly coupled to the workspace and the binary is distributed as an AUR package + GitHub release tarball instead.

### Manual flow

```sh
# 1. Verify each crate packs cleanly. The first one needs no network deps.
cargo package -p wisp-platform --no-verify

# 2. Publish in order (the workflow does the same with 45s sleeps).
cargo publish -p wisp-platform
sleep 45  # let the index pick it up
cargo publish -p wisp-core
sleep 45
cargo publish -p wisp-cleaners
```

### Via GitHub Actions

The `Publish to crates.io` workflow (manual `workflow_dispatch`) runs the same flow with a `dry_run` toggle and a per-crate target. It needs `secrets.CARGO_REGISTRY_TOKEN`.

> **Heads-up:** `wisp-core` may already exist on crates.io under another owner — check with `cargo search wisp-core` before publishing. If the name is taken, pick a unique prefix (e.g. `wispclean-core`) and update the manifests / `[workspace.dependencies]`.
