# wisp-core

L2 of the [`wisp`](https://github.com/chiiydd/wisp) workspace — file-system primitives, scanning, sizing, blacklist, trash and cross-cutting types.

This crate is consumed by `wisp-cleaners` (L3) and `wisp-engine` (L4); it has no UI or CLI dependencies and can be reused on its own when you only need safe FS scanning + deletion plumbing.

See the [main README](https://github.com/chiiydd/wisp#readme) for the full picture and the [architecture doc](https://github.com/chiiydd/wisp/blob/master/docs/architecture.md) for the layer contract.

## License

Dual-licensed under MIT or Apache-2.0, at your option.
