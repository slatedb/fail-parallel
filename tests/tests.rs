// Copyright 2019 TiKV Project Authors. Licensed under Apache-2.0.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::*;
use std::time::*;
use std::*;

use fail::{fail_point, FailPointRegistry};

#[test]
fn test_off() {
    let registry = Arc::new(FailPointRegistry::new());
    let f = || {
        fail_point!(registry.clone(), "off", |_| 2);
        0
    };
    assert_eq!(f(), 0);

    fail::cfg(registry.clone(), "off", "off").unwrap();
    assert_eq!(f(), 0);
}

#[test]
#[cfg_attr(not(feature = "failpoints"), ignore)]
fn test_return() {
    let registry = Arc::new(FailPointRegistry::new());
    let f = || {
        fail_point!(registry.clone(), "return", |s: Option<String>| s
            .map_or(2, |s| s.parse().unwrap()));
        0
    };
    assert_eq!(f(), 0);

    fail::cfg(registry.clone(), "return", "return(1000)").unwrap();
    assert_eq!(f(), 1000);

    fail::cfg(registry.clone(), "return", "return").unwrap();
    assert_eq!(f(), 2);
}

#[test]
#[cfg_attr(not(feature = "failpoints"), ignore)]
fn test_sleep() {
    let registry = Arc::new(FailPointRegistry::new());
    let f = || {
        fail_point!(registry.clone(), "sleep");
    };
    let timer = Instant::now();
    f();
    assert!(timer.elapsed() < Duration::from_millis(1000));

    let timer = Instant::now();
    fail::cfg(registry.clone(), "sleep", "sleep(1000)").unwrap();
    f();
    assert!(timer.elapsed() > Duration::from_millis(1000));
}

#[test]
#[should_panic]
#[cfg_attr(not(feature = "failpoints"), ignore)]
fn test_panic() {
    let registry = Arc::new(FailPointRegistry::new());
    let f = || {
        fail_point!(registry.clone(), "panic");
    };
    fail::cfg(registry.clone(), "panic", "panic(msg)").unwrap();
    f();
}

#[test]
#[cfg_attr(not(feature = "failpoints"), ignore)]
fn test_print() {
    let registry = Arc::new(FailPointRegistry::new());
    struct LogCollector(Arc<Mutex<Vec<String>>>);
    impl log::Log for LogCollector {
        fn enabled(&self, _: &log::Metadata) -> bool {
            true
        }
        fn log(&self, record: &log::Record) {
            let mut buf = self.0.lock().unwrap();
            buf.push(format!("{}", record.args()));
        }
        fn flush(&self) {}
    }

    let buffer = Arc::new(Mutex::new(vec![]));
    let collector = LogCollector(buffer.clone());
    log::set_max_level(log::LevelFilter::Info);
    log::set_boxed_logger(Box::new(collector)).unwrap();

    let f = || {
        fail_point!(registry.clone(), "print");
    };
    fail::cfg(registry.clone(), "print", "print(msg)").unwrap();
    f();
    let msg = buffer.lock().unwrap().pop().unwrap();
    assert_eq!(msg, "msg");

    fail::cfg(registry.clone(), "print", "print").unwrap();
    f();
    let msg = buffer.lock().unwrap().pop().unwrap();
    assert_eq!(msg, "failpoint print executed.");
}

#[test]
#[cfg_attr(not(feature = "failpoints"), ignore)]
fn test_pause() {
    let registry = Arc::new(FailPointRegistry::new());
    let f_registry = registry.clone();
    let f = move || {
        fail_point!(f_registry.clone(), "pause");
    };
    f();

    fail::cfg(registry.clone(), "pause", "pause").unwrap();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        // pause
        f();
        tx.send(()).unwrap();
        // woken up by new order pause, and then pause again.
        f();
        tx.send(()).unwrap();
        // woken up by remove, and then quit immediately.
        f();
        tx.send(()).unwrap();
    });

    assert!(rx.recv_timeout(Duration::from_millis(500)).is_err());
    fail::cfg(registry.clone(), "pause", "pause").unwrap();
    rx.recv_timeout(Duration::from_millis(500)).unwrap();

    assert!(rx.recv_timeout(Duration::from_millis(500)).is_err());
    fail::remove(registry.clone(), "pause");
    rx.recv_timeout(Duration::from_millis(500)).unwrap();

    rx.recv_timeout(Duration::from_millis(500)).unwrap();
}

#[test]
fn test_yield() {
    let registry = Arc::new(FailPointRegistry::new());
    let f = || {
        fail_point!(registry.clone(), "yield");
    };
    fail::cfg(registry.clone(), "test", "yield").unwrap();
    f();
}

#[test]
#[cfg_attr(not(feature = "failpoints"), ignore)]
fn test_callback() {
    let registry = Arc::new(FailPointRegistry::new());
    let f1 = || {
        fail_point!(registry.clone(), "cb");
    };
    let f2 = || {
        fail_point!(registry.clone(), "cb");
    };

    let counter = Arc::new(AtomicUsize::new(0));
    let counter2 = counter.clone();
    fail::cfg_callback(registry.clone(), "cb", move || {
        counter2.fetch_add(1, Ordering::SeqCst);
    })
    .unwrap();
    f1();
    f2();
    assert_eq!(2, counter.load(Ordering::SeqCst));
}

#[test]
#[cfg_attr(not(feature = "failpoints"), ignore)]
fn test_delay() {
    let registry = Arc::new(FailPointRegistry::new());
    let f = || fail_point!(registry.clone(), "delay");
    let timer = Instant::now();
    fail::cfg(registry.clone(), "delay", "delay(1000)").unwrap();
    f();
    assert!(timer.elapsed() > Duration::from_millis(1000));
}

#[test]
#[cfg_attr(not(feature = "failpoints"), ignore)]
fn test_freq_and_count() {
    let registry = Arc::new(FailPointRegistry::new());
    let f = || {
        fail_point!(registry.clone(), "freq_and_count", |s: Option<String>| s
            .map_or(2, |s| s.parse().unwrap()));
        0
    };
    fail::cfg(
        registry.clone(),
        "freq_and_count",
        "50%50*return(1)->50%50*return(-1)->50*return",
    )
    .unwrap();
    let mut sum = 0;
    for _ in 0..5000 {
        let res = f();
        sum += res;
    }
    assert_eq!(sum, 100);
}

#[test]
#[cfg_attr(not(feature = "failpoints"), ignore)]
fn test_condition() {
    let registry = Arc::new(FailPointRegistry::new());
    let f = |_enabled| {
        fail_point!(registry.clone(), "condition", _enabled, |_| 2);
        0
    };
    assert_eq!(f(false), 0);

    fail::cfg(registry.clone(), "condition", "return").unwrap();
    assert_eq!(f(false), 0);

    assert_eq!(f(true), 2);
}

#[test]
fn test_list() {
    let registry = Arc::new(FailPointRegistry::new());
    assert!(!fail::list(registry.clone()).contains(&("list".to_string(), "off".to_string())));
    fail::cfg(registry.clone(), "list", "off").unwrap();
    assert!(fail::list(registry.clone()).contains(&("list".to_string(), "off".to_string())));
    fail::cfg(registry.clone(), "list", "return").unwrap();
    assert!(fail::list(registry.clone()).contains(&("list".to_string(), "return".to_string())));
}
