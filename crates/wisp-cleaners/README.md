# wisp-cleaners

L3 of the [`wisp`](https://github.com/chiiydd/wisp) workspace — concrete cleaner implementations (pacman, paccache, journal, /tmp, browser caches, cargo, npm, pip, go, flatpak, docker, …).

Each cleaner declares *what* to clean via the `CleanerMeta` trait and is auto-registered with `linkme`; actual file-system work is delegated to `wisp-core`.

See the [main README](https://github.com/chiiydd/wisp#readme) and [adding a cleaner](https://github.com/chiiydd/wisp/blob/master/docs/adding-a-cleaner.md).

## License

Dual-licensed under MIT or Apache-2.0, at your option.
