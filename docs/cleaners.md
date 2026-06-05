# Cleaners

This catalog lists the cleaners registered in the current workspace. CLI targets
can be one of the group selectors (`@system`, `@user`, `@dev`, `@all`) or an
individual cleaner ID. Short suffixes such as `pacman` also resolve when they are
unambiguous.

| ID | Group | Risk | Action Type | Needs Root | Description |
| -- | -- | -- | -- | -- | -- |
| arch.journal | System | Safe | external | yes | Vacuum systemd journal files. |
| arch.pacman | System | Safe | delete | yes | Clean old pacman package cache entries, keeping recent versions. |
| arch.orphans | System | Moderate | external | yes | Remove Arch orphan packages. |
| system.tmp | System | Dangerous | delete | no | Clean selected `/tmp` children. |
| user.thumbnails | User | Trivial | delete | no | Remove thumbnail cache files. |
| user.common_cache | User | Trivial | delete | no | Remove common rebuildable desktop caches. |
| user.browser_cache | User | Trivial | delete | no | Remove browser rebuildable cache directories. |
| user.browser_state | User | Dangerous | delete | no | Remove browser site/session state while preserving passwords, bookmarks, and history. |
| user.trash | User | Safe | delete | no | Empty user trash files and metadata. |
| user.flatpak | User | Moderate | external | no | Uninstall unused Flatpak runtimes and extensions. |
| user.linuxqq_cache | User | Safe | delete | no | Remove LinuxQQ logs and rebuildable caches. |
| user.linuxqq_media | User | Dangerous | delete | no | Remove LinuxQQ media cache directories. |
| dev.cargo | Dev | Safe | delete | no | Remove Cargo registry and git caches. |
| dev.npm | Dev | Safe | external | no | Clean npm cache. |
| dev.javascript | Dev | Safe | delete | no | Remove JavaScript toolchain package and build caches. |
| dev.pip | Dev | Safe | delete | no | Remove pip HTTP and wheel cache. |
| dev.python_extra | Dev | Safe | delete | no | Remove Python tool caches beyond pip. |
| dev.go | Dev | Safe | delete | no | Remove Go module cache. |
| dev.docker | Dev | Moderate | external | no | Prune Docker dangling images and build-cache data. |

Action types map to `CleanAction` variants:

- `delete`: wisp deletes the planned path through the engine, using trash or
  direct deletion according to the active config and CLI flags.
- `external`: wisp runs a bounded external command through the engine.

Risk tiers are intentionally conservative. `Dangerous` cleaners require an
explicit apply command (`-y` / `--yes`) and are meant for data that may be hard
to rebuild.
