/*!
A fast and ergonomic concurrent hash-table for read-heavy workloads.

# Features

- An ergonomic lock-free API — no more deadlocks!
- Powerful atomic operations.
- Seamless usage in async contexts.
- Extremely scalable, low-latency reads (see [performance](#performance)).
- Predictable latency across all operations.
- Efficient memory usage, with garbage collection powered by [`seize`].

# Overview

The top-level crate documentation is organized as follows:

- [Usage](#usage) shows how to interact with the concurrent `HashMap`.
- [Consistency](#consistency) describes the guarantees of concurrent operations.
- [Atomic Operations](#atomic-operations) shows how to perform dynamic operations atomically.
- [Async Support](#async-support) shows how to use the map in an async context.
- [Advanced Lifetimes](#advanced-lifetimes) explains how to use guards when working with nested types.
- [Performance](#performance) provides details of expected performance characteristics.

# Usage

`papaya` aims to provide an ergonomic API without sacrificing performance. [`HashMap`] exposes a lock-free API, enabling it to hand out direct references to objects in the map without the need for wrapper types that are clunky and prone to deadlocks. However, you can't hold on to references forever due to concurrent removals. Because of this, the `HashMap` API is structured around *pinning*. Through a pin you can access the map just like a standard `HashMap`. A pin is similar to a lock guard, so any references that are returned will be tied to the lifetime of the guard. Unlike a lock however, pinning is cheap and can never cause deadlocks.

```rust
use papaya::HashMap;

// Create a map.
let map = HashMap::new();

// Pin the map.
let map = map.pin();

// Use the map as normal.
map.insert('A', 1);
assert_eq!(map.get(&'A'), Some(&1));
assert_eq!(map.len(), 1);
```

As expected of a concurrent `HashMap`, all operations take a shared reference. This allows the map to be freely pinned and accessed from multiple threads:

```rust
use papaya::HashMap;

// Use a map from multiple threads.
let map = HashMap::new();
std::thread::scope(|s| {
    // Insert some values.
    s.spawn(|| {
        let map = map.pin();
        for i in 'A'..='Z' {
            map.insert(i, 1);
        }
    });

    // Remove the values.
    s.spawn(|| {
        let map = map.pin();
        for i in 'A'..='Z' {
            map.remove(&i);
        }
    });

    // Read the values.
    s.spawn(|| {
        for (key, value) in map.pin().iter() {
            println!("{key}: {value}");
        }
    });
});
```

It is important to note that as long as you are holding on to a guard, you are preventing the map from performing garbage collection. Pinning and unpinning the table is relatively cheap but not free, similar to the cost of locking and unlocking an uncontended or lightly contended `Mutex`. Thus guard reuse is encouraged, within reason. See the [`seize`] crate for advanced usage and specifics of the garbage collection algorithm.

# Consistency

Due to the concurrent nature of the map, read and write operations may overlap in time. There is no support for locking the entire table nor individual keys to prevent concurrent access, except through external fine-grained locking. As such, read operations (such as `get`) reflect the results of the *most-recent* write. More formally, a read establishes a *happens-before* relationship with the corresponding write.

Aggregate operations, such as iterators, rely on a weak snapshot of the table and return results reflecting the state of the table at or some point after the creation of the iterator. This means that they may, but are not guaranteed to, reflect concurrent modifications to the table that occur during iteration. Similarly, operations such as `clear` and `clone` rely on iteration and may not produce "perfect" results if the map is being concurrently modified.

Note that to obtain a stable snapshot of the table, aggregate table operations require completing any in-progress resizes. If you rely heavily on iteration or similar operations you should consider configuring [`ResizeMode::Blocking`].

# Atomic Operations

As mentioned above, `papaya` does not support locking keys to prevent access, which makes performing complex operations more challenging. Instead, `papaya` exposes a number of atomic operations. The most basic of these is [`HashMap::update`], which can be used to update an existing value in the map using a closure:

```rust
let map = papaya::HashMap::new();
map.pin().insert("poneyland", 42);
assert_eq!(map.pin().update("poneyland", |e| e + 1), Some(&43));
```

Note that in the event that the entry is concurrently modified during an `update`, the closure may be called multiple times to retry the operation. For this reason, update operations are intended to be quick and *pure*, as they may be retried or internally memoized.

`papaya` also exposes more powerful atomic operations that serve as a replacement for the [standard entry API](std::collections::hash_map::Entry). These include:

- [`HashMap::update`]
- [`HashMap::update_or_insert`]
- [`HashMap::update_or_insert_with`]
- [`HashMap::get_or_insert`]
- [`HashMap::get_or_insert_with`]
- [`HashMap::compute`]

For example, with a standard `HashMap`, `Entry::and_modify` is often paired with `Entry::or_insert`:

```rust
use std::collections::HashMap;

let mut map = HashMap::new();
// Insert `poneyland` with the value `42` if it doesn't exist,
// otherwise increment it's value.
map.entry("poneyland")
   .and_modify(|e| { *e += 1 })
   .or_insert(42);
```

However, implementing this with a concurrent `HashMap` is tricky as the entry may be modified in-between operations. Instead, you can write the above operation using [`HashMap::update_or_insert`]:

```rust
use papaya::HashMap;

let map = HashMap::new();
// Insert `poneyland` with the value `42` if it doesn't exist,
// otherwise increment it's value.
map.pin().update_or_insert("poneyland", |e| e + 1, 42);
```

Atomic operations are extremely powerful but also easy to misuse. They may be less efficient than update mechanisms tailored for the specific type of data in the map. For example, concurrent counters should avoid using `update` and instead use `AtomicUsize`. Entries that are frequently modified may also benefit from fine-grained locking.

# Async Support

By default, a pinned map guard does not implement `Send` as it is tied to the current thread, similar to a lock guard. This leads to an issue in work-stealing schedulers as guards are not valid across `.await` points.

To overcome this, you can use an *owned* guard.

```rust
# use std::sync::Arc;
use papaya::HashMap;

async fn run(map: Arc<HashMap<i32, String>>) {
    tokio::spawn(async move {
        // Pin the map with an owned guard.
        let map = map.pin_owned();

        // Hold references across await points.
        let value = map.get(&37);
        tokio::fs::write("db.txt", format!("{value:?}")).await;
        println!("{value:?}");
    });
}
```

Note that owned guards are more expensive to create than regular guards, so they should only be used if necessary. In the above example, you could instead drop the reference and call `get` a second time after the asynchronous call. A more fitting example involves asynchronous iteration:

```rust
# use std::sync::Arc;
use papaya::HashMap;

async fn run(map: Arc<HashMap<i32, String>>) {
    tokio::spawn(async move {
        for (key, value) in map.pin_owned().iter() {
            tokio::fs::write("db.txt", format!("{key}: {value}\n")).await;
        }
    });
}
```

# Advanced Lifetimes

You may run into issues when you try to return a reference to a map contained within an outer type. For example:

```rust,compile_fail
pub struct Metrics {
    map: papaya::HashMap<String, Vec<u64>>
}

impl Metrics {
    pub fn get(&self, name: &str) -> Option<&[u64]> {
        // error[E0515]: cannot return value referencing temporary value
        Some(self.map.pin().get(name)?.as_slice())
    }
}
```

The solution is to accept a guard in the method directly, tying the lifetime to the caller's stack frame:

```rust
use papaya::Guard;

pub struct Metrics {
    map: papaya::HashMap<String, Vec<u64>>
}

impl Metrics {
    pub fn guard(&self) -> impl Guard + '_ {
        self.map.guard()
    }

    pub fn get<'guard>(&self, name: &str, guard: &'guard impl Guard) -> Option<&'guard [u64]> {
        Some(self.map.get(name, guard)?.as_slice())
    }
}
```

The `Guard` trait supports both local and owned guards. Note the `'guard` lifetime that ties the guard to the returned reference. No wrapper types or guard mapping is necessary.

# Performance

`papaya` is built with read-heavy workloads in mind. As such, read operations are extremely high throughput and provide consistent performance that scales with concurrency, meaning `papaya` will excel in workloads where reads are more common than writes. In write heavy workloads, `papaya` will still provide competitive performance despite not being it's primary use case. See the [benchmarks] for details.

`papaya` aims to provide predictable and consistent latency across all operations. Most operations are lock-free, and those that aren't only block under rare and constrained conditions. `papaya` also features [incremental resizing](ResizeMode). Predictable latency is an important part of performance that doesn't often show up in benchmarks, but has significant implications for real-world usage.

[benchmarks]: https://github.com/ibraheemdev/papaya/blob/master/BENCHMARKS.md
*/

#![deny(missing_debug_implementations, missing_docs, dead_code)]
// We use some polyfills for unstable APIs related to strict-provenance.
#![allow(unstable_name_collisions)]
// Stylistic preferences.
#![allow(clippy::multiple_bound_locations, clippy::single_match)]

mod map;
mod raw;

#[cfg(feature = "serde")]
mod serde_impls;

pub use map::{
    Compute, HashMap, HashMapBuilder, HashMapRef, Iter, Keys, OccupiedError, Operation, ResizeMode,
    Values,
};
pub use seize::{Guard, LocalGuard, OwnedGuard};
