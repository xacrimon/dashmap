# ClashMap

> Conrad Ludgate's Dashmap Fork:
> Removes much of the unsafe from dashmap, and improves on the API

Sharded hashmap suitable for fast concurrent access.

ClashMap is an implementation of a concurrent associative array/hashmap in Rust.

ClashMap tries to implement an easy to use API similar to `std::collections::HashMap`
with some slight changes to handle concurrency.

ClashMap tries to be very simple to use and to be a direct replacement for `RwLock<HashMap<K, V>>`.
To accomplish these goals, all methods take `&self` instead of modifying methods taking `&mut self`.
This allows you to put a ClashMap in an `Arc<T>` and share it between threads while still being able to modify it.

ClashMap puts great effort into performance and aims to be as fast as possible.
If you have any suggestions or tips do not hesitate to open an issue or a PR.

[![version](https://img.shields.io/crates/v/clashmap)](https://crates.io/crates/clashmap)

[![documentation](https://docs.rs/clashmap/badge.svg)](https://docs.rs/clashmap)

[![downloads](https://img.shields.io/crates/d/clashmap)](https://crates.io/crates/clashmap)

[![minimum rustc version](https://img.shields.io/badge/rustc-1.65-orange.svg)](https://crates.io/crates/clashmap)

## Cargo features

- `serde` - Enables serde support.

- `raw-api` - Enables the unstable raw-shard api.

- `rayon` - Enables rayon support.

- `inline` - Enables `inline-more` feature from the `hashbrown` crate. Can lead to better performance, but with the cost of longer compile-time.

## Contributing

ClashMap gladly accepts contributions!
Do not hesitate to open issues or PR's.

I will take a look as soon as I have time for it.

That said I do not get paid (yet) to work on open-source. This means
that my time is limited and my work here comes after my personal life.

## Performance

A comprehensive benchmark suite, not yet including ClashMap, can be found [here](https://github.com/xacrimon/conc-map-bench).

## Special thanks

- [Joel Wejdenstål](https://github.com/xacrimon)

- [Jon Gjengset](https://github.com/jonhoo)

- [Yato](https://github.com/RustyYato)

- [Karl Bergström](https://github.com/kabergstrom)

- [Dylan DPC](https://github.com/Dylan-DPC)

- [Lokathor](https://github.com/Lokathor)

- [namibj](https://github.com/namibj)

## License

This project is licensed under MIT.
