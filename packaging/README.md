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

## Distribution channels

`wisp` ships via two channels, both anchored to GitHub Releases:

1. **AUR** — `wisp` (stable tarball) and `wisp-git` (VCS). See [aur/README.md](aur/README.md) for the push flow.
2. **Pre-built tarball** — attached directly to each GitHub Release.

The library crates (`wisp-platform`, `wisp-core`, `wisp-cleaners`, `wisp-engine`, `wisp-tui`) are **not** published to crates.io; they're internal to the workspace and the only artifact is the `wisp` binary.
