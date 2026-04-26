# AUR packaging

Two PKGBUILDs are kept under version control as the source-of-truth, and pushed
to the AUR via `git subtree push` (or by hand) when releasing.

| Directory     | AUR package | Source                                |
| ------------- | ----------- | ------------------------------------- |
| `wisp/`       | `wisp`      | GitHub release tarball, tag `vX.Y.Z`  |
| `wisp-git/`   | `wisp-git`  | git+https://github.com/chiiydd/wisp.git |

## Releasing `wisp` (stable)

1. Tag the release on GitHub (`vX.Y.Z`). The `release` workflow uploads a tarball.
2. Bump `pkgver` in `wisp/PKGBUILD` and reset `pkgrel=1`.
3. Replace the `SKIP` checksum with the real `sha256sum` of the tagged tarball:
   ```sh
   curl -L https://github.com/chiiydd/wisp/archive/refs/tags/vX.Y.Z.tar.gz \
     | sha256sum
   ```
4. Run `makepkg --printsrcinfo > .SRCINFO` and commit both files.
5. Push to AUR:
   ```sh
   git -C /tmp/aur-wisp pull --rebase
   cp PKGBUILD .SRCINFO /tmp/aur-wisp/
   git -C /tmp/aur-wisp commit -am "wisp X.Y.Z-1"
   git -C /tmp/aur-wisp push
   ```

## Releasing `wisp-git`

`wisp-git` rebuilds from `master` on every install — there's no version to bump.
Only push if the build inputs change (e.g. new `depends`, new completions).
Regenerate `.SRCINFO` and push.
