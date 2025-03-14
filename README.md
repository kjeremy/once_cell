<p align="center"><img src="design/logo.png" alt="once_cell" height="300px"></p>


[![Build Status](https://travis-ci.org/matklad/once_cell.svg?branch=master)](https://travis-ci.org/matklad/once_cell)
[![Crates.io](https://img.shields.io/crates/v/once_cell.svg)](https://crates.io/crates/once_cell)
[![API reference](https://docs.rs/once_cell/badge.svg)](https://docs.rs/once_cell/)

# Overview

`once_cell` provides two new cell-like types, `unsync::OnceCell` and `sync::OnceCell`. `OnceCell`
might store arbitrary non-`Copy` types, can be assigned to at most once and provide direct access
to the stored contents. In a nutshell, API looks *roughly* like this:

```rust,ignore
impl OnceCell<T> {
    fn set(&self, value: T) -> Result<(), T> { ... }
    fn get(&self) -> Option<&T> { ... }
}
```

Note that, like with `RefCell` and `Mutex`, the `set` method requires only a shared reference.
Because of the single assignment restriction `get` can return an `&T` instead of `Ref<T>`
or `MutexGuard<T>`.

# Patterns

`OnceCell` might be useful for a variety of patterns.

## Safe Initialization of global data


```rust
use std::{env, io};

use once_cell::sync::OnceCell;

#[derive(Debug)]
pub struct Logger {
    // ...
}
static INSTANCE: OnceCell<Logger> = OnceCell::new();

impl Logger {
    pub fn global() -> &'static Logger {
        INSTANCE.get().expect("logger is not initialized")
    }

    fn from_cli(args: env::Args) -> Result<Logger, std::io::Error> {
       // ...
#      Ok(Logger {})
    }
}

fn main() {
    let logger = Logger::from_cli(env::args()).unwrap();
    INSTANCE.set(logger).unwrap();
    // use `Logger::global()` from now on
}
```

## Lazy initialized global data

This is essentially `lazy_static!` macro, but without a macro.

```rust
use std::{sync::Mutex, collections::HashMap};

use once_cell::sync::OnceCell;

fn global_data() -> &'static Mutex<HashMap<i32, String>> {
    static INSTANCE: OnceCell<Mutex<HashMap<i32, String>>> = OnceCell::new();
    INSTANCE.get_or_init(|| {
        let mut m = HashMap::new();
        m.insert(13, "Spica".to_string());
        m.insert(74, "Hoyten".to_string());
        Mutex::new(m)
    })
}
```

There are also `sync::Lazy` and `unsync::Lazy` convenience types to streamline this pattern:

```rust
use std::{sync::Mutex, collections::HashMap};
use once_cell::sync::Lazy;

static GLOBAL_DATA: Lazy<Mutex<HashMap<i32, String>>> = Lazy::new(|| {
    let mut m = HashMap::new();
    m.insert(13, "Spica".to_string());
    m.insert(74, "Hoyten".to_string());
    Mutex::new(m)
});

fn main() {
    println!("{:?}", GLOBAL_DATA.lock().unwrap());
}
```

## General purpose lazy evaluation

Unlike `lazy_static!`, `Lazy` works with local variables.

```rust
use once_cell::unsync::Lazy;

fn main() {
    let ctx = vec![1, 2, 3];
    let thunk = Lazy::new(|| {
        ctx.iter().sum::<i32>()
    });
    assert_eq!(*thunk, 6);
}
```

If you need a lazy field in a struct, you probably should use `OnceCell`
directly, because that will allow you to access `self` during initialization.

```rust
use std::{fs, path::PathBuf};

use once_cell::unsync::OnceCell;

struct Ctx {
    config_path: PathBuf,
    config: OnceCell<String>,
}

impl Ctx {
    pub fn get_config(&self) -> Result<&str, std::io::Error> {
        let cfg = self.config.get_or_try_init(|| {
            fs::read_to_string(&self.config_path)
        })?;
        Ok(cfg.as_str())
    }
}
```

# Comparison with std

|`!Sync` types         | Access Mode            | Drawbacks                                     |
|----------------------|------------------------|-----------------------------------------------|
|`Cell<T>`             | `T`                    | works only with `Copy` types                  |
|`RefCel<T>`           | `RefMut<T>` / `Ref<T>` | may panic at runtime                          |
|`unsync::OnceCell<T>` | `&T`                   | assignable only once                          |

|`Sync` types          | Access Mode            | Drawbacks                                     |
|----------------------|------------------------|-----------------------------------------------|
|`AtomicT`             | `T`                    | works only with certain `Copy` types          |
|`Mutex<T>`            | `MutexGuard<T>`        | may deadlock at runtime, may block the thread |
|`sync::OnceCell<T>`   | `&T`                   | assignable only once, may block the thread    |

Technically, calling `get_or_init` will also cause a panic or a deadlock if it recursively calls
itself. However, because the assignment can happen only once, such cases should be more rare than
equivalents with `RefCell` and `Mutex`.

# Minimum Supported `rustc` Version

This crate's minimum supported `rustc` version is `1.31.1`.

If optional features are not enabled (`default-features = false` in `Cargo.toml`),
MSRV will be updated conservatively. When using specific features or default features, MSRV might be updated
more frequently, up to the latest stable. In both cases, increasing MSRV is not considered a semver-breaking
change.

# Implementation details

Implementation is based on [`lazy_static`](https://github.com/rust-lang-nursery/lazy-static.rs/) and
[`lazy_cell`](https://github.com/indiv0/lazycell/) crates and in some sense just streamlines and
unifies the APIs of those crates.

To implement a sync flavor of `OnceCell`, this crates uses either `std::sync::Once` or
`parking_lot::Mutex`. This is controlled by the `parking_lot` feature, which is enabled by default.

This crate uses unsafe.

# Related crates

* [double-checked-cell](https://github.com/niklasf/double-checked-cell)
* [lazy-init](https://crates.io/crates/lazy-init)
* [lazycell](https://crates.io/crates/lazycell)
* [mitochondria](https://crates.io/crates/mitochondria)
* [lazy_static](https://crates.io/crates/lazy_static)
