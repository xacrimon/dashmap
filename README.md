# DashMap

DashMap is a blazing fast implementation of a concurrent key -> value map.

DashMap tries to implement an easy to use API while also having more advanced interfaces
for squeezing out performance.

DashMap puts great effort into performance and aims to be as fast as possible.
If you have any suggestions or tips do not hesitate to open an issue or a PR.

[Documentation](https://docs.rs/dashmap)

[![version](https://img.shields.io/crates/v/dashmap)](https://crates.io/crates/dashmap)

[![downloads](https://img.shields.io/crates/d/dashmap)](https://crates.io/crates/dashmap)

## Contributing

DashMap is gladly accepts contributions!
Do not hesitate to open issues or PR's.

## Performance

DashMap is included in a set of benchmarks found [here](https://git.nebulanet.cc/Acrimon/conc-map-bench)
that use [bustle](https://docs.rs/bustle/0.3.2/bustle), a port of the libcuckoo benchmark harness.
Benchmarks are a best effort and we try to make them as unbiased and realistic as possible. Contributions are accepted there too!

## Special thanks

- [Jon Gjengset](https://github.com/jonhoo)

- [Krishna Sannasi](https://github.com/KrishnaSannasi) 

- [Karl Bergström](https://github.com/kabergstrom)

- [Dylan DPC](https://github.com/Dylan-DPC)

- Jon Gjengset

- Karl Bergström

- [namibj](https://github.com/namibj)

## License

This project is licensed under MIT.
