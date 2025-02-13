# DashMap

Blazingly fast concurrent map in Rust.

DashMap is an implementation of a concurrent associative array/hashmap in Rust.

DashMap tries to implement an easy to use API similar to `std::collections::HashMap`
with some slight changes to handle concurrency.

DashMap tries to be very simple to use and to be a direct replacement for `RwLock<HashMap<K, V>>`.
To accomplish these goals, all methods take `&self` instead of modifying methods taking `&mut self`.
This allows you to put a DashMap in an `Arc<T>` and share it between threads while still being able to modify it.

DashMap puts great effort into performance and aims to be as fast as possible.
If you have any suggestions or tips do not hesitate to open an issue or a PR.

The current MSRV is 1.70 and is not changed in patch releases. You can pin a minor version if you want
perfect stability. Though `dashmap` always stays at least 15 full stable releases (or about 9 months) behind the current stable release.

[![version](https://img.shields.io/crates/v/dashmap)](https://crates.io/crates/dashmap)

[![documentation](https://docs.rs/dashmap/badge.svg)](https://docs.rs/dashmap)

[![downloads](https://img.shields.io/crates/d/dashmap)](https://crates.io/crates/dashmap)

[![minimum rustc version](https://img.shields.io/badge/rustc-1.70-orange.svg)](https://crates.io/crates/dashmap)

## Cargo features

- `serde` - Enables serde support.

- `raw-api` - Enables the unstable raw-shard api.

- `rayon` - Enables rayon support.

- `inline-more` - Enables `inline-more` feature from the `hashbrown` crate. Comes with the usual tradeoffs of possibly excessive inlining.

- `arbitrary` - Enables support for the `arbitrary` crate.

## Contributing

DashMap gladly accepts contributions!
Do not hesitate to open issues or PR's.

I will take a look as soon as I have time for it.

That said I do not get paid (yet) to work on open-source. This means
that my time is limited and my work here comes after my personal life.

## Performance

A comprehensive benchmark suite including DashMap can be found [here](https://github.com/xacrimon/conc-map-bench).

## Special thanks

- [Conrad Ludgate](https://github.com/conradludgate)

- [Jon Gjengset](https://github.com/jonhoo)

- [Yato](https://github.com/RustyYato)

- [Karl Bergström](https://github.com/kabergstrom)

- [Dylan DPC](https://github.com/Dylan-DPC)

- [Lokathor](https://github.com/Lokathor)

- [namibj](https://github.com/namibj)

## License

This project is licensed under MIT.
