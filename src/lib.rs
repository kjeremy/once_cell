/*!
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

*/

#[cfg(feature = "parking_lot")]
#[path = "imp_pl.rs"]
mod imp;
#[cfg(not(feature = "parking_lot"))]
#[path = "imp_std.rs"]
mod imp;

pub mod unsync {
    use std::{
        ops::Deref,
        cell::UnsafeCell,
        panic::{UnwindSafe, RefUnwindSafe},
    };

    /// A cell which can be written to only once. Not thread safe.
    ///
    /// Unlike `::std::cell::RefCell`, a `OnceCell` provides simple `&`
    /// references to the contents.
    ///
    /// # Example
    /// ```
    /// use once_cell::unsync::OnceCell;
    ///
    /// let cell = OnceCell::new();
    /// assert!(cell.get().is_none());
    ///
    /// let value: &String = cell.get_or_init(|| {
    ///     "Hello, World!".to_string()
    /// });
    /// assert_eq!(value, "Hello, World!");
    /// assert!(cell.get().is_some());
    /// ```
    #[derive(Debug)]
    pub struct OnceCell<T> {
        // Invariant: written to at most once.
        inner: UnsafeCell<Option<T>>,
    }

    impl<T: RefUnwindSafe + UnwindSafe> RefUnwindSafe for OnceCell<T> {}
    impl<T: UnwindSafe> UnwindSafe for OnceCell<T> {}

    impl<T> Default for OnceCell<T> {
        fn default() -> Self {
            Self::new()
        }
    }

    impl<T: Clone> Clone for OnceCell<T> {
        fn clone(&self) -> OnceCell<T> {
            let res = OnceCell::new();
            if let Some(value) = self.get() {
                match res.set(value.clone()) {
                    Ok(()) => (),
                    Err(_) => unreachable!(),
                }
            }
            res
        }
    }

    impl<T: PartialEq> PartialEq for OnceCell<T> {
        fn eq(&self, other: &Self) -> bool {
            self.get() == other.get()
        }
    }

    impl<T> From<T> for OnceCell<T> {
        fn from(value: T) -> Self {
            OnceCell { inner: UnsafeCell::new(Some(value)) }
        }
    }

    impl<T> OnceCell<T> {
        /// Creates a new empty cell.
        pub const fn new() -> OnceCell<T> {
            OnceCell { inner: UnsafeCell::new(None) }
        }

        /// Gets the reference to the underlying value. Returns `None`
        /// if the cell is empty.
        pub fn get(&self) -> Option<&T> {
            // Safe due to `inner`'s invariant
            unsafe { &*self.inner.get() }.as_ref()
        }

        /// Sets the contents of this cell to `value`. Returns
        /// `Ok(())` if the cell was empty and `Err(value)` if it was
        /// full.
        ///
        /// # Example
        /// ```
        /// use once_cell::unsync::OnceCell;
        ///
        /// let cell = OnceCell::new();
        /// assert!(cell.get().is_none());
        ///
        /// assert_eq!(cell.set(92), Ok(()));
        /// assert_eq!(cell.set(62), Err(62));
        ///
        /// assert!(cell.get().is_some());
        /// ```
        pub fn set(&self, value: T) -> Result<(), T> {
            let slot = unsafe { &*self.inner.get() };
            if slot.is_some() {
                return Err(value);
            }
            let slot = unsafe { &mut*self.inner.get() };
            // This is the only place where we set the slot, no races
            // due to reentrancy/concurrency are possible, and we've
            // checked that slot is currently `None`, so this write
            // maintains the `inner`'s invariant.
            *slot = Some(value);
            Ok(())
        }

        /// Gets the contents of the cell, initializing it with `f`
        /// if the cell was empty.
        ///
        /// # Panics
        ///
        /// If `f` panics, the panic is propagated to the caller, and the cell
        /// remains uninitialized.
        ///
        /// It is an error to reentrantly initialize the cell from `f`. Doing
        /// so results in a panic.
        ///
        /// # Example
        /// ```
        /// use once_cell::unsync::OnceCell;
        ///
        /// let cell = OnceCell::new();
        /// let value = cell.get_or_init(|| 92);
        /// assert_eq!(value, &92);
        /// let value = cell.get_or_init(|| unreachable!());
        /// assert_eq!(value, &92);
        /// ```
        pub fn get_or_init<F: FnOnce() -> T>(&self, f: F) -> &T {
            enum Void {}
            match self.get_or_try_init(|| Ok::<T, Void>(f())) {
                Ok(val) => val,
                Err(void) => match void {},
            }
        }

        /// Gets the contents of the cell, initializing it with `f` if
        /// the cell was empty. If the cell was empty and `f` failed, an
        /// error is returned.
        ///
        /// # Panics
        ///
        /// If `f` panics, the panic is propagated to the caller, and the cell
        /// remains uninitialized.
        ///
        /// It is an error to reentrantly initialize the cell from `f`. Doing
        /// so results in a panic.
        ///
        /// # Example
        /// ```
        /// use once_cell::unsync::OnceCell;
        ///
        /// let cell = OnceCell::new();
        /// assert_eq!(cell.get_or_try_init(|| Err(())), Err(()));
        /// assert!(cell.get().is_none());
        /// let value = cell.get_or_try_init(|| -> Result<i32, ()> {
        ///     Ok(92)
        /// });
        /// assert_eq!(value, Ok(&92));
        /// assert_eq!(cell.get(), Some(&92))
        /// ```
        pub fn get_or_try_init<F: FnOnce() -> Result<T, E>, E>(&self, f: F) -> Result<&T, E> {
            if let Some(val) = self.get() {
                return Ok(val);
            }
            let val = f()?;
            assert!(self.set(val).is_ok(), "reentrant init");
            Ok(self.get().unwrap())
        }

        /// Consumes the `OnceCell`, returning the wrapped value. Returns
        /// `None` if the cell was empty.
        ///
        /// # Examples
        ///
        /// ```
        /// use once_cell::unsync::OnceCell;
        ///
        /// let cell: OnceCell<String> = OnceCell::new();
        /// assert_eq!(cell.into_inner(), None);
        ///
        /// let cell = OnceCell::new();
        /// cell.set("hello".to_string()).unwrap();
        /// assert_eq!(cell.into_inner(), Some("hello".to_string()));
        /// ```
        pub fn into_inner(self) -> Option<T> {
            // Because `into_inner` takes `self` by value, the compiler statically verifies
            // that it is not currently borrowed. So it is safe to move out `Option<T>`.
            self.inner.into_inner()
        }
    }

    /// A value which is initialized on the first access.
    ///
    /// # Example
    /// ```
    /// use once_cell::unsync::Lazy;
    ///
    /// let lazy: Lazy<i32> = Lazy::new(|| {
    ///     println!("initializing");
    ///     92
    /// });
    /// println!("ready");
    /// println!("{}", *lazy);
    /// println!("{}", *lazy);
    ///
    /// // Prints:
    /// //   ready
    /// //   initializing
    /// //   92
    /// //   92
    /// ```
    #[derive(Debug)]
    pub struct Lazy<T, F = fn() -> T> {
        cell: OnceCell<T>,
        init: F,
    }

    impl<T, F> Lazy<T, F> {
        /// Creates a new lazy value with the given initializing function.
        ///
        /// # Example
        /// ```
        /// # extern crate once_cell;
        /// # fn main() {
        /// use once_cell::unsync::Lazy;
        ///
        /// let hello = "Hello, World!".to_string();
        ///
        /// let lazy = Lazy::new(|| hello.to_uppercase());
        ///
        /// assert_eq!(&*lazy, "HELLO, WORLD!");
        /// # }
        /// ```
        pub const fn new(init: F) -> Lazy<T, F> {
            Lazy { cell: OnceCell::new(), init }
        }
    }

    impl<T, F: Fn() -> T> Lazy<T, F> {
        /// Forces the evaluation of this lazy value and
        /// returns a reference to result. This is equivalent
        /// to the `Deref` impl, but is explicit.
        ///
        /// # Example
        /// ```
        /// use once_cell::unsync::Lazy;
        ///
        /// let lazy = Lazy::new(|| 92);
        ///
        /// assert_eq!(Lazy::force(&lazy), &92);
        /// assert_eq!(&*lazy, &92);
        /// ```
        pub fn force(this: &Lazy<T, F>) -> &T {
            this.cell.get_or_init(|| (this.init)())
        }
    }

    impl<T, F: Fn() -> T> Deref for Lazy<T, F> {
        type Target = T;
        fn deref(&self) -> &T {
            Lazy::force(self)
        }
    }
}

pub mod sync {
    use crate::imp::OnceCell as Imp;

    /// A thread-safe cell which can be written to only once.
    ///
    /// Unlike `::std::sync::Mutex`, a `OnceCell` provides simple `&`
    /// references to the contents.
    ///
    /// # Example
    /// ```
    /// use once_cell::sync::OnceCell;
    ///
    /// static CELL: OnceCell<String> = OnceCell::new();
    /// assert!(CELL.get().is_none());
    ///
    /// std::thread::spawn(|| {
    ///     let value: &String = CELL.get_or_init(|| {
    ///         "Hello, World!".to_string()
    ///     });
    ///     assert_eq!(value, "Hello, World!");
    /// }).join().unwrap();
    ///
    /// let value: Option<&String> = CELL.get();
    /// assert!(value.is_some());
    /// assert_eq!(value.unwrap().as_str(), "Hello, World!");
    /// ```
    #[derive(Debug)]
    pub struct OnceCell<T>(Imp<T>);

    impl<T> Default for OnceCell<T> {
        fn default() -> OnceCell<T> {
            OnceCell::new()
        }
    }

    impl<T: Clone> Clone for OnceCell<T> {
        fn clone(&self) -> OnceCell<T> {
            let res = OnceCell::new();
            if let Some(value) = self.get() {
                match res.set(value.clone()) {
                    Ok(()) => (),
                    Err(_) => unreachable!(),
                }
            }
            res
        }
    }

    impl<T> From<T> for OnceCell<T> {
        fn from(value: T) -> Self {
            let cell = Self::new();
            cell.get_or_init(|| value);
            cell
        }
    }

    impl<T: PartialEq> PartialEq for OnceCell<T> {
        fn eq(&self, other: &OnceCell<T>) -> bool {
            self.get() == other.get()
        }
    }

    impl<T> OnceCell<T> {
        /// Creates a new empty cell.
        pub const fn new() -> OnceCell<T> {
            OnceCell(Imp::new())
        }

        /// Gets the reference to the underlying value. Returns `None`
        /// if the cell is empty, or being initialized. This method does
        /// not block.
        pub fn get(&self) -> Option<&T> {
            self.0.get()
        }

        /// Sets the contents of this cell to `value`. Returns
        /// `Ok(())` if the cell was empty and `Err(value)` if it was
        /// full.
        ///
        /// # Example
        /// ```
        /// use once_cell::sync::OnceCell;
        ///
        /// static CELL: OnceCell<i32> = OnceCell::new();
        ///
        /// fn main() {
        ///     assert!(CELL.get().is_none());
        ///
        ///     std::thread::spawn(|| {
        ///         assert_eq!(CELL.set(92), Ok(()));
        ///     }).join().unwrap();
        ///
        ///     assert_eq!(CELL.set(62), Err(62));
        ///     assert_eq!(CELL.get(), Some(&92));
        /// }
        /// ```
        pub fn set(&self, value: T) -> Result<(), T> {
            self.0.set(value)
        }

        /// Gets the contents of the cell, initializing it with `f`
        /// if the cell was empty. May threads may call `get_or_init`
        /// concurrently with different initializing functions, but
        /// it is guaranteed that only one function will be executed.
        ///
        /// # Panics
        ///
        /// If `f` panics, the panic is propagated to the caller, and
        /// the cell remains uninitialized.
        ///
        /// It is an error to reentrantly initialize the cell from `f`.
        /// The exact outcome is unspecified. Current implementation
        /// deadlocks, but this may be changed to a panic in the future.
        ///
        /// # Example
        /// ```
        /// use once_cell::sync::OnceCell;
        ///
        /// let cell = OnceCell::new();
        /// let value = cell.get_or_init(|| 92);
        /// assert_eq!(value, &92);
        /// let value = cell.get_or_init(|| unreachable!());
        /// assert_eq!(value, &92);
        /// ```
        pub fn get_or_init<F: FnOnce() -> T>(&self, f: F) -> &T {
            self.0.get_or_init(f)
        }

        /// Gets the contents of the cell, initializing it with `f` if
        /// the cell was empty. If the cell was empty and `f` failed, an
        /// error is returned.
        ///
        /// Note that this method requires `parking_lot` Cargo feature.
        ///
        /// # Panics
        ///
        /// If `f` panics, the panic is propagated to the caller, and
        /// the cell remains uninitialized.
        ///
        /// It is an error to reentrantly initialize the cell from `f`.
        /// The exact outcome is unspecified. Current implementation
        /// deadlocks, but this may be changed to a panic in the future.
        ///
        /// # Example
        /// ```
        /// use once_cell::sync::OnceCell;
        ///
        /// let cell = OnceCell::new();
        /// assert_eq!(cell.get_or_try_init(|| Err(())), Err(()));
        /// assert!(cell.get().is_none());
        /// let value = cell.get_or_try_init(|| -> Result<i32, ()> {
        ///     Ok(92)
        /// });
        /// assert_eq!(value, Ok(&92));
        /// assert_eq!(cell.get(), Some(&92))
        /// ```
        #[cfg(feature = "parking_lot")]
        pub fn get_or_try_init<F: FnOnce() -> Result<T, E>, E>(&self, f: F) -> Result<&T, E> {
            self.0.get_or_try_init(f)
        }

        /// Consumes the `OnceCell`, returning the wrapped value. Returns
        /// `None` if the cell was empty.
        ///
        /// # Examples
        ///
        /// ```
        /// use once_cell::sync::OnceCell;
        ///
        /// let cell: OnceCell<String> = OnceCell::new();
        /// assert_eq!(cell.into_inner(), None);
        ///
        /// let cell = OnceCell::new();
        /// cell.set("hello".to_string()).unwrap();
        /// assert_eq!(cell.into_inner(), Some("hello".to_string()));
        /// ```
        pub fn into_inner(self) -> Option<T> {
            self.0.into_inner()
        }
    }

    /// A value which is initialized on the first access.
    ///
    /// This type is thread-safe and can be used in statics:
    ///
    /// # Example
    /// ```
    /// extern crate once_cell;
    ///
    /// use std::collections::HashMap;
    /// use once_cell::sync::Lazy;
    ///
    /// static HASHMAP: Lazy<HashMap<i32, String>> = Lazy::new(|| {
    ///     println!("initializing");
    ///     let mut m = HashMap::new();
    ///     m.insert(13, "Spica".to_string());
    ///     m.insert(74, "Hoyten".to_string());
    ///     m
    /// });
    ///
    /// fn main() {
    ///     println!("ready");
    ///     std::thread::spawn(|| {
    ///         println!("{:?}", HASHMAP.get(&13));
    ///     }).join().unwrap();
    ///     println!("{:?}", HASHMAP.get(&74));
    ///
    ///     // Prints:
    ///     //   ready
    ///     //   initializing
    ///     //   Some("Spica")
    ///     //   Some("Hoyten")
    /// }
    /// ```
    #[derive(Debug)]
    pub struct Lazy<T, F = fn() -> T> {
        cell: OnceCell<T>,
        init: F,
    }

    impl<T, F> Lazy<T, F> {
        /// Creates a new lazy value with the given initializing
        /// function.
        pub const fn new(f: F) -> Lazy<T, F> {
            Lazy { cell: OnceCell::new(), init: f }
        }
    }

    impl<T, F: Fn() -> T> Lazy<T, F> {
        /// Forces the evaluation of this lazy value and
        /// returns a reference to result. This is equivalent
        /// to the `Deref` impl, but is explicit.
        ///
        /// # Example
        /// ```
        /// use once_cell::sync::Lazy;
        ///
        /// let lazy = Lazy::new(|| 92);
        ///
        /// assert_eq!(Lazy::force(&lazy), &92);
        /// assert_eq!(&*lazy, &92);
        /// ```
        pub fn force(this: &Lazy<T, F>) -> &T {
            this.cell.get_or_init(|| (this.init)())
        }
    }

    impl<T, F: Fn() -> T> ::std::ops::Deref for Lazy<T, F> {
        type Target = T;
        fn deref(&self) -> &T {
            Lazy::force(self)
        }
    }
}
