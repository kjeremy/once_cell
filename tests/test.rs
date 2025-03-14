use std::{
    mem, thread, ptr,
    cell::Cell,
    sync::atomic::{AtomicUsize, Ordering::SeqCst},
    sync::Barrier,
};

use crossbeam_utils::thread::scope;

use once_cell::{sync, unsync};

fn go<F: FnOnce() -> ()>(mut f: F) {
    struct Yolo<T>(T);
    unsafe impl<T> Send for Yolo<T> {}

    let ptr: *const u8 = &mut f as *const F as *const u8;
    mem::forget(f);
    let yolo = Yolo(ptr);
    thread::spawn(move || {
        let f: F = unsafe { ptr::read(yolo.0 as *const F) };
        f();
    })
    .join()
    .unwrap();
}

#[test]
fn unsync_once_cell() {
    let c = unsync::OnceCell::new();
    assert!(c.get().is_none());
    c.get_or_init(|| 92);
    assert_eq!(c.get(), Some(&92));

    c.get_or_init(|| panic!("Kabom!"));
    assert_eq!(c.get(), Some(&92));
}

#[test]
fn sync_once_cell() {
    let c = sync::OnceCell::new();
    assert!(c.get().is_none());
    go(|| {
        c.get_or_init(|| 92);
        assert_eq!(c.get(), Some(&92));
    });
    c.get_or_init(|| panic!("Kabom!"));
    assert_eq!(c.get(), Some(&92));
}

#[test]
fn unsync_once_cell_drop() {
    static DROP_CNT: AtomicUsize = AtomicUsize::new(0);
    struct Dropper;
    impl Drop for Dropper {
        fn drop(&mut self) {
            DROP_CNT.fetch_add(1, SeqCst);
        }
    }

    let x = unsync::OnceCell::new();
    x.get_or_init(|| Dropper);
    assert_eq!(DROP_CNT.load(SeqCst), 0);
    drop(x);
    assert_eq!(DROP_CNT.load(SeqCst), 1);
}

#[test]
fn sync_once_cell_drop() {
    static DROP_CNT: AtomicUsize = AtomicUsize::new(0);
    struct Dropper;
    impl Drop for Dropper {
        fn drop(&mut self) {
            DROP_CNT.fetch_add(1, SeqCst);
        }
    }

    let x = sync::OnceCell::new();
    go(|| {
        x.get_or_init(|| Dropper);
        assert_eq!(DROP_CNT.load(SeqCst), 0);
        drop(x);
    });
    assert_eq!(DROP_CNT.load(SeqCst), 1);
}

#[test]
fn unsync_once_cell_drop_empty() {
    let x = unsync::OnceCell::<String>::new();
    drop(x);
}

#[test]
fn sync_once_cell_drop_empty() {
    let x = sync::OnceCell::<String>::new();
    drop(x);
}

#[test]
fn unsync_lazy_new() {
    let called = Cell::new(0);
    let x = unsync::Lazy::new(|| {
        called.set(called.get() + 1);
        92
    });

    assert_eq!(called.get(), 0);

    let y = *x - 30;
    assert_eq!(y, 62);
    assert_eq!(called.get(), 1);

    let y = *x - 30;
    assert_eq!(y, 62);
    assert_eq!(called.get(), 1);
}

#[test]
fn sync_lazy_new() {
    let called = AtomicUsize::new(0);
    let x = sync::Lazy::new(|| {
        called.fetch_add(1, SeqCst);
        92
    });

    assert_eq!(called.load(SeqCst), 0);

    go(|| {
        let y = *x - 30;
        assert_eq!(y, 62);
        assert_eq!(called.load(SeqCst), 1);
    });

    let y = *x - 30;
    assert_eq!(y, 62);
    assert_eq!(called.load(SeqCst), 1);
}

#[test]
fn static_lazy() {
    static XS: sync::Lazy<Vec<i32>> = sync::Lazy::new(|| {
        let mut xs = Vec::new();
        xs.push(1);
        xs.push(2);
        xs.push(3);
        xs
    });
    go(|| {
        assert_eq!(&*XS, &vec![1, 2, 3]);
    });
    assert_eq!(&*XS, &vec![1, 2, 3]);
}

#[test]
fn static_lazy_no_macros() {
    fn xs() -> &'static Vec<i32> {
        static XS: sync::OnceCell<Vec<i32>> = sync::OnceCell::new();
        XS.get_or_init(|| {
            let mut xs = Vec::new();
            xs.push(1);
            xs.push(2);
            xs.push(3);
            xs
        })
    }
    assert_eq!(xs(), &vec![1, 2, 3]);
}

#[test]
fn sync_once_cell_is_sync_send() {
    fn assert_traits<T: Send + Sync>() {}
    assert_traits::<sync::OnceCell<String>>();
}

#[test]
fn eval_once_macro() {
    macro_rules! eval_once {
        (|| -> $ty:ty {
            $($body:tt)*
        }) => {{
            static ONCE_CELL: sync::OnceCell<$ty> = sync::OnceCell::new();
            fn init() -> $ty {
                $($body)*
            }
            ONCE_CELL.get_or_init(init)
        }};
    }

    let fib: &'static Vec<i32> = eval_once! {
        || -> Vec<i32> {
            let mut res = vec![1, 1];
            for i in 0..10 {
                let next = res[i] + res[i + 1];
                res.push(next);
            }
            res
        }
    };
    assert_eq!(fib[5], 8)
}

#[test]
fn sync_once_cell_does_not_leak_partially_constructed_boxes() {
    let n_tries = 100;
    let n_readers = 10;
    let n_writers = 3;
    const MSG: &str = "Hello, World";

    for _ in 0..n_tries {
        let cell: sync::OnceCell<String> = sync::OnceCell::new();
        scope(|scope| {
            for _ in 0..n_readers {
                scope.spawn(|_| loop {
                    if let Some(msg) = cell.get() {
                        assert_eq!(msg, MSG);
                        break;
                    }
                });
            }
            for _ in 0..n_writers {
                scope.spawn(|_| cell.set(MSG.to_owned()));
            }
        })
        .unwrap()
    }
}

#[test]
fn sync_get_does_not_block() {
    let cell = sync::OnceCell::new();
    let barrier = Barrier::new(2);
    scope(|scope| {
        scope.spawn(|_| {
            cell.get_or_init(|| {
                barrier.wait();
                barrier.wait();
                "hello".to_string()
            });
        });
        barrier.wait();
        assert_eq!(cell.get(), None);
        barrier.wait();
    })
    .unwrap();
    assert_eq!(cell.get(), Some(&"hello".to_string()));
}

#[test]
fn unsync_clone() {
    let s = unsync::OnceCell::new();
    let c = s.clone();
    assert!(c.get().is_none());

    s.set("hello".to_string()).unwrap();
    let c = s.clone();
    assert_eq!(c.get().map(String::as_str), Some("hello"));
}

#[test]
fn sync_clone() {
    let s = sync::OnceCell::new();
    let c = s.clone();
    assert!(c.get().is_none());

    s.set("hello".to_string()).unwrap();
    let c = s.clone();
    assert_eq!(c.get().map(String::as_str), Some("hello"));
}

#[test]
#[cfg(feature = "parking_lot")]
fn sync_get_or_try_init() {
    let cell: sync::OnceCell<String> = sync::OnceCell::new();
    assert!(cell.get().is_none());

    let res = std::panic::catch_unwind(|| cell.get_or_try_init(|| -> Result<_, ()> { panic!() }));
    assert!(res.is_err());
    assert!(cell.get().is_none());

    assert_eq!(cell.get_or_try_init(|| Err(())), Err(()));

    assert_eq!(cell.get_or_try_init(|| Ok::<_, ()>("hello".to_string())), Ok(&"hello".to_string()));
    assert_eq!(cell.get(), Some(&"hello".to_string()));
}

#[test]
fn from_impl() {
    assert_eq!(sync::OnceCell::from("value").get(), Some(&"value"));
    assert_eq!(unsync::OnceCell::from("value").get(), Some(&"value"));
    assert_ne!(sync::OnceCell::from("foo").get(), Some(&"bar"));
    assert_ne!(unsync::OnceCell::from("foo").get(), Some(&"bar"));
}

#[test]
fn partialeq_impl() {
    assert!(sync::OnceCell::from("value") == sync::OnceCell::from("value"));
    assert!(sync::OnceCell::from("foo") != sync::OnceCell::from("bar"));
    assert!(unsync::OnceCell::from("value") == unsync::OnceCell::from("value"));
    assert!(unsync::OnceCell::from("foo") != unsync::OnceCell::from("bar"));

    assert!(sync::OnceCell::<String>::new() == sync::OnceCell::new());
    assert!(sync::OnceCell::<String>::new() != sync::OnceCell::from("value".to_owned()));
    assert!(unsync::OnceCell::<String>::new() == unsync::OnceCell::new());
    assert!(unsync::OnceCell::<String>::new() != unsync::OnceCell::from("value".to_owned()));
}

#[test]
fn unsync_into_inner() {
    let cell: unsync::OnceCell<String> = unsync::OnceCell::new();
    assert_eq!(cell.into_inner(), None);
    let cell = unsync::OnceCell::new();
    cell.set("hello".to_string()).unwrap();
    assert_eq!(cell.into_inner(), Some("hello".to_string()));
}

#[test]
fn sync_into_inner() {
    let cell: sync::OnceCell<String> = sync::OnceCell::new();
    assert_eq!(cell.into_inner(), None);
    let cell = sync::OnceCell::new();
    cell.set("hello".to_string()).unwrap();
    assert_eq!(cell.into_inner(), Some("hello".to_string()));
}
