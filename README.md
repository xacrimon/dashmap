# dashmap

Blazingly fast concurrent map in Rust.

DashMap is an implementation of a concurrent associative array/hashmap in Rust.

DashMap tries to implement an easy to use API similar to `std::collections::HashMap`
with some slight changes to handle concurrency.

DashMap tries to be very simple to use and to be a direct replacement for `RwLock<HashMap<K, V>>`.
To accomplish these all methods take `&self` instead modifying methods taking `&mut self`.
This allows you to put a DashMap in an `Arc<T>` and share it between threads while being able to modify it.

DashMap puts great effort into performance and aims to be as fast as possible.
If you have any suggestions or tips do not hesitate to open an issue or a PR.

[![version](https://img.shields.io/crates/v/dashmap)](https://crates.io/crates/dashmap)

[![downloads](https://img.shields.io/crates/d/dashmap)](https://crates.io/crates/dashmap)

## Cargo features

- `nightly` - Enables experimental nightly optimizations.

- `serde` - Enables serde support.

- `raw-api` - Enables the unstable raw-shard api.

## Contributing

DashMap is gladly accepts contributions!
Do not hesitate to open issues or PR's.

I will take a look as soon as I have time for it.

## Performance

Benchmarks are currently not fantastic and can be improved and more can be created.
Help is welcomed with open arms.

<img src="https://raw.githubusercontent.com/xacrimon/dashmap/master/assets/bench-insert.svg?sanitize=true" alt="Insert Benchmark">

<img src="https://raw.githubusercontent.com/xacrimon/dashmap/master/assets/bench-get.svg?sanitize=true" alt="Get Benchmark">

[Google Doc](https://docs.google.com/spreadsheets/d/1q2VR_rMZRzG7YO0ef6V0jMA6hAdkafh_wI8xvY_51fk/edit?usp=sharing)

## Special thanks

- Karl Bergström

- DPC

## License

This project is licensed under MIT.
