# DashMap

DashMap is a blazing fast implementation of a concurrent key -> value map.

DashMap tries to implement an easy to use API while also having more advanced interfaces
for squeezing out performance.

We put great effort into performance and aims to be as fast as possible.
If you have any suggestions or tips do not hesitate to open an issue or a PR.

[Documentation](https://docs.rs/dashmap)

[![version](https://img.shields.io/crates/v/dashmap)](https://crates.io/crates/dashmap)

[![downloads](https://img.shields.io/crates/d/dashmap)](https://crates.io/crates/dashmap)

[![minimum rustc version](https://img.shields.io/badge/rustc-1.44.1-orange.svg)](https://github.com/rust-random/rand#rust-version-requirements)

## Serde support

Turn on the `serde` feature and `DashMap` will implement `Serialize` and `Deserialize`.

## Contributing

DashMap is gladly accepts contributions!
Do not hesitate to open issues or PR's.

## Performance

DashMap is included in a set of benchmarks found [here](https://git.acrimon.dev/Acrimon/conc-map-bench)
that use [bustle](https://docs.rs/bustle), a port of the libcuckoo benchmark harness.
Benchmarks are a best effort and we try to make them as unbiased and realistic as possible. Contributions are accepted there too!

## Support

[![Patreon](https://c5.patreon.com/external/logo/become_a_patron_button@2x.png)](https://patreon.com/acrimon)

Creating and testing open-source software like DashMap takes up a large portion of my time
and comes with costs such as test hardware. Please consider supporting me and everything I make for the public
to enable me to continue doing this.

If you want to support me please head over and take a look at my [patreon](https://www.patreon.com/acrimon).

## Special thanks

- [Jon Gjengset](https://github.com/jonhoo)

- [Krishna Sannasi](https://github.com/KrishnaSannasi) 

- [Karl Bergström](https://github.com/kabergstrom)

- [Dylan DPC](https://github.com/Dylan-DPC)

- [Lokathor](https://github.com/Lokathor)

- [namibj](https://github.com/namibj)

## License

This project is licensed under MIT.
