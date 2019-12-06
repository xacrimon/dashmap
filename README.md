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

[![pipeline](http://gitlab.nebulanet.cc/xacrimon/dashmap/badges/master/pipeline.svg)](http://gitlab.nebulanet.cc/xacrimon/dashmap/commits/master)

[![version](https://img.shields.io/crates/v/dashmap)](https://crates.io/crates/dashmap)

[![downloads](https://img.shields.io/crates/d/dashmap)](https://crates.io/crates/dashmap)

## Contributing

DashMap is gladly accepts contributions!
Do not hesitate to open issues or PR's.

I will take a look as soon as I have time for it.

## Performance

![Insert Benchmark](https://gitlab.nebulanet.cc/xacrimon/dashmap/tree/master/assets/bench-insert.svg "Insert Benchmark")

![Get Benchmark](https://gitlab.nebulanet.cc/xacrimon/dashmap/tree/master/assets/bench-get.svg "Get Benchmark")

[Google Doc](https://docs.google.com/spreadsheets/d/1q2VR_rMZRzG7YO0ef6V0jMA6hAdkafh_wI8xvY_51fk/edit?usp=sharing)

## Special thanks

- Karl Bergström

- DPC

## License

This project is licensed under MIT.
