// Copyright 2019 TiKV Project Authors. Licensed under Apache-2.0.

//! A fail point implementation for Rust.
//!
//! Fail points are code instrumentations that allow errors and other behavior
//! to be injected dynamically at runtime, primarily for testing purposes. Fail
//! points are flexible and can be configured to exhibit a variety of behavior,
//! including panics, early returns, and sleeping. They can be controlled both
//! programmatically and via the environment, and can be triggered
//! conditionally and probabilistically.
//!
//! This crate is inspired by FreeBSD's
//! [failpoints](https://freebsd.org/cgi/man.cgi?query=fail).
//!
//! ## Usage
//!
//! You can import the `fail_point!` macro from this module to inject dynamic failures.
//!
//! As an example, here's a simple program that uses a fail point to simulate an
//! I/O panic:
//!
//! ```rust, ignore
//! use crate::failpoints::{fail_point, FailScenario, FailPointRegistry};
//! use std::sync::Arc;
//!
//! fn do_fallible_work(fp_registry: Arc<FailPointRegistry>) {
//!     fail_point!(fp_registry, "read-dir");
//!     let _dir: Vec<_> = std::fs::read_dir(".").unwrap().collect();
//!     // ... do some work on the directory ...
//! }
//!
//! let registry = Arc::new(FailPointRegistry::new());
//! let scenario = FailScenario::setup(registry.clone());
//! do_fallible_work(fp_registry.clone());
//! scenario.teardown();
//! println!("done");
//! ```
//!
//! Here, the program calls `unwrap` on the result of `read_dir`, a function
//! that returns a `Result`. In other words, this particular program expects
//! this call to `read_dir` to always succeed. And in practice it almost always
//! will, which makes the behavior of this program when `read_dir` fails
//! difficult to test. By instrumenting the program with a fail point we can
//! pretend that `read_dir` failed, causing the subsequent `unwrap` to panic,
//! and allowing us to observe the program's behavior under failure conditions.
//!
//! When the program is run normally it just prints "done":
//!
//! ```sh
//! $ cargo run --features fail/failpoints
//!     Finished dev [unoptimized + debuginfo] target(s) in 0.01s
//!      Running `target/debug/failpointtest`
//! done
//! ```
//!
//! But now, by setting the `FAILPOINTS` variable we can see what happens if the
//! `read_dir` fails:
//!
//! ```sh
//! FAILPOINTS=read-dir=panic cargo run --features fail/failpoints
//!     Finished dev [unoptimized + debuginfo] target(s) in 0.01s
//!      Running `target/debug/failpointtest`
//! thread 'main' panicked at 'failpoint read-dir panic', /home/ubuntu/.cargo/registry/src/github.com-1ecc6299db9ec823/fail-0.2.0/src/lib.rs:286:25
//! note: Run with `RUST_BACKTRACE=1` for a backtrace.
//! ```
//!
//! ## Usage in tests
//!
//! The previous example triggers a fail point by modifying the `FAILPOINTS`
//! environment variable. In practice, you'll often want to trigger fail points
//! programmatically, in unit tests.
//! Fail points are global resources, and Rust tests run in parallel,
//! so tests that exercise fail points generally need to hold a lock to
//! avoid interfering with each other. This is accomplished by `FailScenario`.
//!
//! Here's a basic pattern for writing unit tests tests with fail points:
//!
//! ```rust, ignore,no_run
//! use crate::failpoints::{fail_point, FailScenario, FailPointRegistry};
//! use std::sync::Arc;
//!
//! fn do_fallible_work(fp_registry: Arc<FailPointRegistry>) {
//!     fail_point!(fp_registry, "read-dir");
//!     let _dir: Vec<_> = std::fs::read_dir(".").unwrap().collect();
//!     // ... do some work on the directory ...
//! }
//!
//! #[test]
//! #[should_panic]
//! fn test_fallible_work() {
//!     let fp_registry = Arc::new(FailPointRegistry::new());
//!     fail::cfg(fp_registry.clone(), "read-dir", "panic").unwrap();
//!
//!     do_fallible_work(fp_registry.clone());
//! }
//! ```
//!
//! ## Early return
//!
//! The previous examples illustrate injecting panics via fail points, but
//! panics aren't the only &mdash; or even the most common &mdash; error pattern
//! in Rust. The more common type of error is propagated by `Result` return
//! values, and fail points can inject those as well with "early returns". That
//! is, when configuring a fail point as "return" (as opposed to "panic"), the
//! fail point will immediately return from the function, optionally with a
//! configurable value.
//!
//! The setup for early return requires a slightly diferent invocation of the
//! `fail_point!` macro. To illustrate this, let's modify the `do_fallible_work`
//! function we used earlier to return a `Result`:
//!
//! ```rust, ignore
//! use crate::failpoints::{fail_point, FailScenario};
//! use std::io;
//! use std::sync::Arc;
//!
//! fn do_fallible_work(fp_registry: Arc<FailPointRegistry>) -> io::Result<()> {
//!     fail_point!(fp_registry, "read-dir");
//!     let _dir: Vec<_> = std::fs::read_dir(".")?.collect();
//!     // ... do some work on the directory ...
//!     Ok(())
//! }
//!
//! fn main() -> io::Result<()> {
//!     let fp_registry = Arc::new(FailPointRegistry::new());
//!     do_fallible_work(fp_registry.clone())?;
//!     println!("done");
//!     Ok(())
//! }
//! ```
//!
//! This example has more proper Rust error handling, with no unwraps
//! anywhere. Instead it uses `?` to propagate errors via the `Result` type
//! return values. This is more realistic Rust code.
//!
//! The "read-dir" fail point though is not yet configured to support early
//! return, so if we attempt to configure it to "return", we'll see an error
//! like
//!
//! ```sh
//! $ FAILPOINTS=read-dir=return cargo run --features fail/failpoints
//!     Finished dev [unoptimized + debuginfo] target(s) in 0.13s
//!      Running `target/debug/failpointtest`
//! thread 'main' panicked at 'Return is not supported for the fail point "read-dir"', src/main.rs:7:5
//! note: Run with `RUST_BACKTRACE=1` for a backtrace.
//! ```
//!
//! This error tells us that the "read-dir" fail point is not defined correctly
//! to support early return, and gives us the line number of that fail point.
//! What we're missing in the fail point definition is code describring _how_ to
//! return an error value, and the way we do this is by passing `fail_point!` a
//! closure that returns the same type as the enclosing function.
//!
//! Here's a variation that does so:
//!
//! ```rust, ignore
//! # use std::io;
//! use std::sync::Arc;
//!
//! fn do_fallible_work(fp_registry: Arc<FailPointRegistry>) -> io::Result<()> {
//!     fail::fail_point!(fp_registry, "read-dir", |_| {
//!         Err(io::Error::new(io::ErrorKind::PermissionDenied, "error"))
//!     });
//!     let _dir: Vec<_> = std::fs::read_dir(".")?.collect();
//!     // ... do some work on the directory ...
//!     Ok(())
//! }
//! ```
//!
//! And now if the "read-dir" fail point is configured to "return" we get a
//! different result:
//!
//! ```sh
//! $ FAILPOINTS=read-dir=return cargo run --features fail/failpoints
//!    Compiling failpointtest v0.1.0
//!     Finished dev [unoptimized + debuginfo] target(s) in 2.38s
//!      Running `target/debug/failpointtest`
//! Error: Custom { kind: PermissionDenied, error: StringError("error") }
//! ```
//!
//! This time, `do_fallible_work` returned the error defined in our closure,
//! which propagated all the way up and out of main.
//!
//! ## Advanced usage
//!
//! That's the basics of fail points: defining them with `fail_point!`,
//! configuring them with `FAILPOINTS` and `fail::cfg`, and configuring them to
//! panic and return early. But that's not all they can do. To learn more see
//! the documentation for [`cfg`](fn.cfg.html),
//! [`cfg_callback`](fn.cfg_callback.html) and
//! [`fail_point!`](macro.fail_point.html).
//!
//!
//! ## Usage considerations
//!
//! For most effective fail point usage, keep in mind the following:
//!
//!  - Fail points are disabled by default and can be enabled via the `failpoints`
//!    feature. When failpoints are disabled, no code is generated by the macro.
//!  - Fail points might have the same name, in which case they take the
//!    same actions. Be careful about duplicating fail point names, either within
//!    a single crate, or across multiple crates.

#![deny(missing_docs, missing_debug_implementations)]
#![allow(warnings)]

use std::collections::HashMap;
use std::env::VarError;
use std::fmt::Debug;
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, RwLock, TryLockError};
use std::time::{Duration, Instant};
use std::{env, mem, thread};
use std::sync::atomic::Ordering::Relaxed;

#[derive(Clone)]
struct SyncCallback(Arc<dyn Fn() + Send + Sync>);

impl Debug for SyncCallback {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SyncCallback()")
    }
}

impl PartialEq for SyncCallback {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl SyncCallback {
    #[allow(dead_code)]
    fn new(f: impl Fn() + Send + Sync + 'static) -> SyncCallback {
        SyncCallback(Arc::new(f))
    }

    #[allow(dead_code)]
    fn run(&self) {
        let callback = &self.0;
        callback();
    }
}

/// Supported tasks.
#[derive(Clone, Debug, PartialEq)]
enum Task {
    /// Do nothing.
    Off,
    /// Return the value.
    Return(Option<String>),
    /// Sleep for some milliseconds.
    Sleep(u64),
    /// Panic with the message.
    Panic(Option<String>),
    /// Print the message.
    Print(Option<String>),
    /// Sleep until other action is set.
    Pause,
    /// Yield the CPU.
    Yield,
    /// Busy waiting for some milliseconds.
    Delay(u64),
    /// Call callback function.
    #[allow(dead_code)]
    Callback(SyncCallback),
}

#[derive(Debug)]
struct Action {
    task: Task,
    freq: f32,
    count: Option<AtomicUsize>,
}

impl PartialEq for Action {
    fn eq(&self, hs: &Action) -> bool {
        if self.task != hs.task || self.freq != hs.freq {
            return false;
        }
        if let Some(ref lhs) = self.count {
            if let Some(ref rhs) = hs.count {
                return lhs.load(Ordering::Relaxed) == rhs.load(Ordering::Relaxed);
            }
        } else if hs.count.is_none() {
            return true;
        }
        false
    }
}

impl Action {
    fn new(task: Task, freq: f32, max_cnt: Option<usize>) -> Action {
        Action {
            task,
            freq,
            count: max_cnt.map(AtomicUsize::new),
        }
    }

    #[allow(dead_code)]
    fn from_callback(f: impl Fn() + Send + Sync + 'static) -> Action {
        let task = Task::Callback(SyncCallback::new(f));
        Action {
            task,
            freq: 1.0,
            count: None,
        }
    }

    #[allow(dead_code)]
    fn get_task(&self) -> Option<Task> {
        use rand::Rng;

        if let Some(ref cnt) = self.count {
            let c = cnt.load(Ordering::Acquire);
            if c == 0 {
                return None;
            }
        }
        if self.freq < 1f32 && !rand::thread_rng().gen_bool(f64::from(self.freq)) {
            return None;
        }
        if let Some(ref ref_cnt) = self.count {
            let mut cnt = ref_cnt.load(Ordering::Acquire);
            loop {
                if cnt == 0 {
                    return None;
                }
                let new_cnt = cnt - 1;
                match ref_cnt.compare_exchange_weak(
                    cnt,
                    new_cnt,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                ) {
                    Ok(_) => break,
                    Err(c) => cnt = c,
                }
            }
        }
        Some(self.task.clone())
    }
}

fn partition(s: &str, pattern: char) -> (&str, Option<&str>) {
    let mut splits = s.splitn(2, pattern);
    (splits.next().unwrap(), splits.next())
}

impl FromStr for Action {
    type Err = String;

    /// Parse an action.
    ///
    /// `s` should be in the format `[p%][cnt*]task[(args)]`, `p%` is the frequency,
    /// `cnt` is the max times the action can be triggered.
    fn from_str(s: &str) -> Result<Action, String> {
        let mut remain = s.trim();
        let mut args = None;
        // in case there is '%' in args, we need to parse it first.
        let (first, second) = partition(remain, '(');
        if let Some(second) = second {
            remain = first;
            if !second.ends_with(')') {
                return Err("parentheses do not match".to_owned());
            }
            args = Some(&second[..second.len() - 1]);
        }

        let mut frequency = 1f32;
        let (first, second) = partition(remain, '%');
        if let Some(second) = second {
            remain = second;
            match first.parse::<f32>() {
                Err(e) => return Err(format!("failed to parse frequency: {}", e)),
                Ok(freq) => frequency = freq / 100.0,
            }
        }

        let mut max_cnt = None;
        let (first, second) = partition(remain, '*');
        if let Some(second) = second {
            remain = second;
            match first.parse() {
                Err(e) => return Err(format!("failed to parse count: {}", e)),
                Ok(cnt) => max_cnt = Some(cnt),
            }
        }

        let parse_timeout = || match args {
            None => Err("sleep require timeout".to_owned()),
            Some(timeout_str) => match timeout_str.parse() {
                Err(e) => Err(format!("failed to parse timeout: {}", e)),
                Ok(timeout) => Ok(timeout),
            },
        };

        let task = match remain {
            "off" => Task::Off,
            "return" => Task::Return(args.map(str::to_owned)),
            "sleep" => Task::Sleep(parse_timeout()?),
            "panic" => Task::Panic(args.map(str::to_owned)),
            "print" => Task::Print(args.map(str::to_owned)),
            "pause" => Task::Pause,
            "yield" => Task::Yield,
            "delay" => Task::Delay(parse_timeout()?),
            _ => return Err(format!("unrecognized command {:?}", remain)),
        };

        Ok(Action::new(task, frequency, max_cnt))
    }
}

#[derive(Debug)]
struct FailPoint {
    actions: Mutex<ConfiguredActions>,
    sync_notifier: Condvar,
    async_notifier: AsyncNotifier
}

#[derive(Debug)]
struct AsyncNotifier {
    tx: tokio::sync::watch::Sender<u64>,
    rx: tokio::sync::watch::Receiver<u64>,
}

#[derive(Debug)]
struct ConfiguredActions {
    seq: u64,
    actions_str: String,
    actions: Vec<Action>
}

impl ConfiguredActions {
    fn empty(seq: u64) -> ConfiguredActions {
        ConfiguredActions {
            seq,
            actions_str: String::new(),
            actions: vec![]
        }
    }
}

impl AsyncNotifier {
    fn new() -> AsyncNotifier {
        let (tx, rx) = tokio::sync::watch::channel(0);
        AsyncNotifier { tx, rx }
    }
}

impl FailPoint {
    #[allow(dead_code)]
    fn new() -> FailPoint {
        let initial_seq: u64 = 0;
        let initial_actions = ConfiguredActions::empty(initial_seq);

        FailPoint {
            actions: Mutex::new(initial_actions),
            sync_notifier: Condvar::new(),
            async_notifier: AsyncNotifier::new(),
        }
    }

    fn actions_str(&self) -> String {
        let actions_guard = self.actions.lock().unwrap();
        (*actions_guard).actions_str.clone()
    }

    fn set_actions(&self, actions_str: &str, actions: Vec<Action>) {
        let mut actions_guard = self.actions.lock().unwrap();
        let next_seq = (*actions_guard).seq + 1;
        *actions_guard = ConfiguredActions {
            seq: next_seq,
            actions_str: actions_str.to_string(),
            actions
        };
        self.sync_notifier.notify_all();
        self.async_notifier.tx.send(next_seq).unwrap();
    }

    #[allow(dead_code)]
    #[allow(clippy::option_option)]
    fn eval(&self, name: &str) -> Option<Option<String>> {
        let (task_opt, action_seq) = self.next_task();
        if let Some(task) = task_opt {
            self.eval_task(action_seq, name, task)
        } else {
            None
        }
    }

    fn eval_task(
        &self,
        action_seq: u64,
        name: &str,
        task: Task
    ) -> Option<Option<String>> {
        match task {
            Task::Off => {}
            Task::Return(s) => return Some(s),
            Task::Sleep(t) => thread::sleep(Duration::from_millis(t)),
            Task::Panic(msg) => match msg {
                Some(ref msg) => panic!("{}", msg),
                None => panic!("failpoint {} panic", name),
            },
            Task::Print(msg) => match msg {
                Some(ref msg) => log::info!("{}", msg),
                None => log::info!("failpoint {} executed.", name),
            },
            Task::Pause => {
                let _unused = self.sync_notifier.wait_while(
                    self.actions.lock().unwrap(),
                    |guard| { (*guard).seq == action_seq }
                ).unwrap();
            },
            Task::Yield => thread::yield_now(),
            Task::Delay(t) => {
                let timer = Instant::now();
                let timeout = Duration::from_millis(t);
                while timer.elapsed() < timeout {}
            }
            Task::Callback(f) => {
                f.run();
            }
        }
        None
    }

    #[allow(dead_code)]
    #[allow(clippy::option_option)]
    async fn eval_async(&self, name: &str) -> Option<Option<String>> {
        let (task_opt, action_seq) = self.next_task();
        if let Some(task) = task_opt {
            self.eval_task_async(action_seq, name, task).await
        } else {
            None
        }
    }

    fn next_task(&self) -> (Option<Task>, u64){
        let guard = self.actions.lock().unwrap();
        let task = guard.actions.iter().filter_map(Action::get_task).next();
        (task, (*guard).seq)
    }

    async fn eval_task_async(
        &self,
        action_seq: u64,
        name: &str,
        task: Task
    ) -> Option<Option<String>> {
        match task {
            Task::Off => {}
            Task::Return(s) => return Some(s),
            Task::Sleep(t) =>
                tokio::time::sleep(Duration::from_millis(t)).await,
            Task::Panic(msg) => match msg {
                Some(ref msg) => panic!("{}", msg),
                None => panic!("failpoint {} panic", name),
            },
            Task::Print(msg) => match msg {
                Some(ref msg) => log::info!("{}", msg),
                None => log::info!("failpoint {} executed.", name),
            },
            Task::Pause => {
                let mut rx = self.async_notifier.rx.clone();
                rx.wait_for(|val| *val != action_seq).await.unwrap();
            },
            Task::Yield => tokio::task::yield_now().await,
            Task::Delay(t) => {
                let timer = Instant::now();
                let timeout = Duration::from_millis(t);
                while timer.elapsed() < timeout {}
            }
            Task::Callback(f) => {
                f.run();
            }
        }
        None
    }
}

/// Registry with failpoints configuration.
type Registry = HashMap<String, Arc<FailPoint>>;

/// A public failpoint registry that's meant to be used in tests.
#[derive(Debug, Default)]
pub struct FailPointRegistry {
    // TODO: remove rwlock or store *mut FailPoint
    registry: RwLock<Registry>,
}

impl FailPointRegistry {
    /// Create a new fail point registry.
    pub fn new() -> Self {
        Self {
            registry: RwLock::new(Registry::new()),
        }
    }
}

/// Test scenario with configured fail points.
#[derive(Debug)]
pub struct FailScenario {
    fp_registry: Arc<FailPointRegistry>,
}

impl FailScenario {
    /// Set up the system for a fail points scenario.
    ///
    /// Configures all fail points specified in the `FAILPOINTS` environment variable.
    /// It does not otherwise change any existing fail point configuration.
    ///
    /// The format of `FAILPOINTS` is `failpoint=actions;...`, where
    /// `failpoint` is the name of the fail point. For more information
    /// about fail point actions see the [`cfg`](fn.cfg.html) function and
    /// the [`fail_point`](macro.fail_point.html) macro.
    ///
    /// `FAILPOINTS` may configure fail points that are not actually defined. In
    /// this case the configuration has no effect.
    ///
    /// This function should generally be called prior to running a test with fail
    /// points, and afterward paired with [`teardown`](#method.teardown).
    ///
    /// # Panics
    ///
    /// Panics if an action is not formatted correctly.
    pub fn setup(fp_registry: Arc<FailPointRegistry>) -> Self {
        // Cleanup first, in case of previous failed/panic'ed test scenarios.
        let mut registry = fp_registry.registry.write().unwrap();
        Self::cleanup(&mut registry);

        let failpoints = match env::var("FAILPOINTS") {
            Ok(s) => s,
            Err(VarError::NotPresent) => {
                return Self {
                    fp_registry: fp_registry.clone(),
                }
            }
            Err(e) => panic!("invalid failpoints: {:?}", e),
        };
        for mut cfg in failpoints.trim().split(';') {
            cfg = cfg.trim();
            if cfg.is_empty() {
                continue;
            }
            let (name, order) = partition(cfg, '=');
            match order {
                None => panic!("invalid failpoint: {:?}", cfg),
                Some(order) => {
                    if let Err(e) = set(&mut registry, name.to_owned(), order) {
                        panic!("unable to configure failpoint \"{}\": {}", name, e);
                    }
                }
            }
        }
        Self {
            fp_registry: fp_registry.clone(),
        }
    }

    /// Tear down the fail point system.
    ///
    /// Clears the configuration of all fail points. Any paused fail
    /// points will be notified before they are deactivated.
    ///
    /// This function should generally be called after running a test with fail points.
    /// Calling `teardown` without previously calling `setup` results in a no-op.
    pub fn teardown(self) {
        drop(self)
    }

    /// Clean all registered fail points.
    fn cleanup(registry: &mut std::sync::RwLockWriteGuard<Registry>) {
        for p in registry.values() {
            // wake up all pause failpoint.
            p.set_actions("", vec![]);
        }
        registry.clear();
    }
}

impl Drop for FailScenario {
    fn drop(&mut self) {
        let mut registry = self.fp_registry.registry.write().unwrap();
        Self::cleanup(&mut registry)
    }
}

/// Returns whether code generation for failpoints is enabled.
///
/// This function allows consumers to check (at runtime) whether the library
/// was compiled with the (buildtime) `failpoints` feature, which enables
/// code generation for failpoints.
pub const fn has_failpoints() -> bool {
    cfg!(feature = "failpoints")
}

/// Get all registered fail points.
///
/// Return a vector of `(name, actions)` pairs.
pub fn list(fp_registry: Arc<FailPointRegistry>) -> Vec<(String, String)> {
    let registry = fp_registry.registry.read().unwrap();
    registry
        .iter()
        .map(|(name, fp)| (name.to_string(), fp.actions_str()))
        .collect()
}

fn find_fail_point(
    fp_registry: Arc<FailPointRegistry>,
    name: &str,
) -> Option<Arc<FailPoint>> {
    let registry = fp_registry.registry.read().unwrap();
    registry.get(name).map(|p| p.clone())
}

#[doc(hidden)]
pub fn eval<R, F: FnOnce(Option<String>) -> R>(
    fp_registry: Arc<FailPointRegistry>,
    name: &str,
    f: F,
) -> Option<R> {
    if let Some(p) = find_fail_point(fp_registry, name) {
        p.eval(name).map(f)
    } else {
        None
    }
}

#[doc(hidden)]
pub async fn eval_async<R, F: FnOnce(Option<String>) -> R>(
    fp_registry: Arc<FailPointRegistry>,
    name: &str,
    f: F,
) -> Option<R> {
    if let Some(p) = find_fail_point(fp_registry, name) {
        p.eval_async(name).await.map(f)
    } else {
        None
    }
}

/// Configure the actions for a fail point at runtime.
///
/// Each fail point can be configured with a series of actions, specified by the
/// `actions` argument. The format of `actions` is `action[->action...]`. When
/// multiple actions are specified, an action will be checked only when its
/// former action is not triggered.
///
/// The format of a single action is `[p%][cnt*]task[(arg)]`. `p%` is the
/// expected probability that the action is triggered, and `cnt*` is the max
/// times the action can be triggered. The supported values of `task` are:
///
/// - `off`, the fail point will do nothing.
/// - `return(arg)`, return early when the fail point is triggered. `arg` is passed to `$e` (
/// defined via the `fail_point!` macro) as a string.
/// - `sleep(milliseconds)`, sleep for the specified time.
/// - `panic(msg)`, panic with the message.
/// - `print(msg)`, log the message, using the `log` crate, at the `info` level.
/// - `pause`, sleep until other action is set to the fail point.
/// - `yield`, yield the CPU.
/// - `delay(milliseconds)`, busy waiting for the specified time.
///
/// For example, `20%3*print(still alive!)->panic` means the fail point has 20% chance to print a
/// message "still alive!" and 80% chance to panic. And the message will be printed at most 3
/// times.
///
/// The `FAILPOINTS` environment variable accepts this same syntax for its fail
/// point actions.
///
/// A call to `cfg` with a particular fail point name overwrites any existing actions for
/// that fail point, including those set via the `FAILPOINTS` environment variable.
pub fn cfg<S: Into<String>>(
    registry: Arc<FailPointRegistry>,
    name: S,
    actions: &str,
) -> Result<(), String> {
    let mut registry = registry.registry.write().unwrap();
    set(&mut registry, name.into(), actions)
}

/// Configure the actions for a fail point at runtime.
///
/// Each fail point can be configured by a callback. Process will call this callback function
/// when it meet this fail-point.
pub fn cfg_callback<S, F>(registry: Arc<FailPointRegistry>, name: S, f: F) -> Result<(), String>
where
    S: Into<String>,
    F: Fn() + Send + Sync + 'static,
{
    let mut registry = registry.registry.write().unwrap();
    let p = registry
        .entry(name.into())
        .or_insert_with(|| Arc::new(FailPoint::new()));
    let action = Action::from_callback(f);
    let actions = vec![action];
    p.set_actions("callback", actions);
    Ok(())
}

/// Remove a fail point.
///
/// If the fail point doesn't exist, nothing will happen.
pub fn remove<S: AsRef<str>>(fp_registry: Arc<FailPointRegistry>, name: S) {
    let mut registry = fp_registry.registry.write().unwrap();
    if let Some(p) = registry.remove(name.as_ref()) {
        // wake up all pause failpoint.
        p.set_actions("", vec![]);
    }
}

/// Configure fail point in RAII style.
#[derive(Debug)]
pub struct FailGuard {
    name: String,
    registry: Arc<FailPointRegistry>,
}

impl Drop for FailGuard {
    fn drop(&mut self) {
        remove(self.registry.clone(), &self.name);
    }
}

impl FailGuard {
    /// Configure the actions for a fail point during the lifetime of the returning `FailGuard`.
    ///
    /// Read documentation of [`cfg`] for more details.
    pub fn new<S: Into<String>>(
        registry: Arc<FailPointRegistry>,
        name: S,
        actions: &str,
    ) -> Result<FailGuard, String> {
        let name = name.into();
        cfg(registry.clone(), &name, actions)?;
        Ok(FailGuard {
            registry: registry.clone(),
            name,
        })
    }

    /// Configure the actions for a fail point during the lifetime of the returning `FailGuard`.
    ///
    /// Read documentation of [`cfg_callback`] for more details.
    pub fn with_callback<S, F>(
        registry: Arc<FailPointRegistry>,
        name: S,
        f: F,
    ) -> Result<FailGuard, String>
    where
        S: Into<String>,
        F: Fn() + Send + Sync + 'static,
    {
        let name = name.into();
        cfg_callback(registry.clone(), &name, f)?;
        Ok(FailGuard {
            registry: registry.clone(),
            name,
        })
    }
}

fn set(registry: &mut HashMap<String, Arc<FailPoint>>,
    name: String,
    actions: &str,
) -> Result<(), String> {
    let actions_str = actions;
    // `actions` are in the format of `failpoint[->failpoint...]`.
    let actions = actions
        .split("->")
        .map(Action::from_str)
        .collect::<Result<_, _>>()?;
    // Please note that we can't figure out whether there is a failpoint named `name`,
    // so we may insert a failpoint that doesn't exist at all.
    let p = registry
        .entry(name)
        .or_insert_with(|| Arc::new(FailPoint::new()));
    p.set_actions(actions_str, actions);
    Ok(())
}

/// Define a fail point (requires `failpoints` feature).
///
/// The `fail_point!` macro has three forms, and they all take a name as the
/// first argument. The simplest form takes only a name and is suitable for
/// executing most fail point behavior, including panicking, but not for early
/// return or conditional execution based on a local flag.
///
/// The three forms of fail points look as follows.
///
/// 1. A basic fail point:
///
/// ```rust, ignore
/// # #[macro_use] extern crate fail;
/// fn function_return_unit() {
///     fail_point!("fail-point-1");
/// }
/// ```
///
/// This form of fail point can be configured to panic, print, sleep, pause, etc., but
/// not to return from the function early.
///
/// 2. A fail point that may return early:
///
/// ```rust, ignore
/// # #[macro_use] extern crate fail;
/// fn function_return_value() -> u64 {
///     fail_point!("fail-point-2", |r| r.map_or(2, |e| e.parse().unwrap()));
///     0
/// }
/// ```
///
/// This form of fail point can additionally be configured to return early from
/// the enclosing function. It accepts a closure, which itself accepts an
/// `Option<String>`, and is expected to transform that argument into the early
/// return value. The argument string is sourced from the fail point
/// configuration string. For example configuring this "fail-point-2" as
/// "return(100)" will execute the fail point closure, passing it a `Some` value
/// containing a `String` equal to "100"; the closure then parses it into the
/// return value.
///
/// 3. A fail point with conditional execution:
///
/// ```rust, ignore
/// # #[macro_use] extern crate fail;
/// fn function_conditional(enable: bool) {
///     fail_point!("fail-point-3", enable, |_| {});
/// }
/// ```
///
/// In this final form, the second argument is a local boolean expression that
/// must evaluate to `true` before the fail point is evaluated. The third
/// argument is again an early-return closure.
///
/// The three macro arguments (or "designators") are called `$name`, `$cond`,
/// and `$e`. `$name` must be `&str`, `$cond` must be a boolean expression,
/// and`$e` must be a function or closure that accepts an `Option<String>` and
/// returns the same type as the enclosing function.
///
/// For more examples see the [crate documentation](index.html). For more
/// information about controlling fail points see the [`cfg`](fn.cfg.html)
/// function.
#[macro_export]
#[cfg(feature = "failpoints")]
macro_rules! fail_point {
    ($registry:expr, $name:expr) => {{
        $crate::eval($registry, $name, |_| {
            panic!("Return is not supported for the fail point \"{}\"", $name);
        });
    }};
    ($registry:expr, $name:expr, $e:expr) => {{
        if let Some(res) = $crate::eval($registry, $name, $e) {
            return res;
        }
    }};
    ($registry:expr, $name:expr, $cond:expr, $e:expr) => {{
        if $cond {
            $crate::fail_point!($registry, $name, $e);
        }
    }};
}

/// Define a fail point (requires `failpoints` feature).
///
/// The `fail_point_async!` macro is similar to `fail_point` except that it
/// can be safely used in an async function. Similar to `fail_point`, it
/// has three forms, and they all take a name as the
/// first argument. The simplest form takes only a name and is suitable for
/// executing most fail point behavior, including panicking, but not for early
/// return or conditional execution based on a local flag.
///
/// The three forms of fail points look as follows.
///
/// 1. A basic fail point:
///
/// ```rust, ignore
/// # #[macro_use] extern crate fail;
/// async fn function_return_unit() {
///     fail_point_async!("fail-point-1");
/// }
/// ```
///
/// This form of fail point can be configured to panic, print, sleep, pause, etc., but
/// not to return from the function early.
///
/// 2. A fail point that may return early:
///
/// ```rust, ignore
/// # #[macro_use] extern crate fail;
/// async fn function_return_value() -> u64 {
///     fail_point_async!("fail-point-2", |r| r.map_or(2, |e| e.parse().unwrap()));
///     0
/// }
/// ```
///
/// This form of fail point can additionally be configured to return early from
/// the enclosing function. It accepts a closure, which itself accepts an
/// `Option<String>`, and is expected to transform that argument into the early
/// return value. The argument string is sourced from the fail point
/// configuration string. For example configuring this "fail-point-2" as
/// "return(100)" will execute the fail point closure, passing it a `Some` value
/// containing a `String` equal to "100"; the closure then parses it into the
/// return value.
///
/// 3. A fail point with conditional execution:
///
/// ```rust, ignore
/// # #[macro_use] extern crate fail;
/// async fn function_conditional(enable: bool) {
///     fail_point_async!("fail-point-3", enable, |_| {});
/// }
/// ```
///
/// In this final form, the second argument is a local boolean expression that
/// must evaluate to `true` before the fail point is evaluated. The third
/// argument is again an early-return closure.
///
/// The three macro arguments (or "designators") are called `$name`, `$cond`,
/// and `$e`. `$name` must be `&str`, `$cond` must be a boolean expression,
/// and`$e` must be a function or closure that accepts an `Option<String>` and
/// returns the same type as the enclosing function.
///
/// For more examples see the [crate documentation](index.html). For more
/// information about controlling fail points see the [`cfg`](fn.cfg.html)
/// function.
#[macro_export]
#[cfg(feature = "failpoints")]
macro_rules! fail_point_async {
    ($registry:expr, $name:expr) => {{
        $crate::eval_async($registry, $name, |_| {
            panic!("Return is not supported for the fail point \"{}\"", $name);
        }).await;
    }};
    ($registry:expr, $name:expr, $e:expr) => {{
        if let Some(res) = $crate::eval_async($registry, $name, $e).await {
            return res;
        }
    }};
    ($registry:expr, $name:expr, $cond:expr, $e:expr) => {{
        if $cond {
            $crate::fail_point_async!($registry, $name, $e);
        }
    }};
}


/// Define a fail point (disabled, see `failpoints` feature).
#[macro_export]
#[cfg(not(feature = "failpoints"))]
macro_rules! fail_point {
    ($registry:expr, $name:expr, $e:expr) => {{}};
    ($registry:expr, $name:expr) => {{}};
    ($registry:expr, $name:expr, $cond:expr, $e:expr) => {{}};
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::*;

    #[test]
    fn test_has_failpoints() {
        assert_eq!(cfg!(feature = "failpoints"), has_failpoints());
    }

    #[test]
    fn test_off() {
        let point = FailPoint::new();
        point.set_actions("", vec![Action::new(Task::Off, 1.0, None)]);
        assert!(point.eval("test_fail_point_off").is_none());
    }

    #[test]
    fn test_return() {
        let point = FailPoint::new();
        point.set_actions("", vec![Action::new(Task::Return(None), 1.0, None)]);
        let res = point.eval("test_fail_point_return");
        assert_eq!(res, Some(None));

        let ret = Some("test".to_owned());
        point.set_actions("", vec![Action::new(Task::Return(ret.clone()), 1.0, None)]);
        let res = point.eval("test_fail_point_return");
        assert_eq!(res, Some(ret));
    }

    #[test]
    fn test_sleep() {
        let point = FailPoint::new();
        let timer = Instant::now();
        point.set_actions("", vec![Action::new(Task::Sleep(1000), 1.0, None)]);
        assert!(point.eval("test_fail_point_sleep").is_none());
        assert!(timer.elapsed() > Duration::from_millis(1000));
    }

    #[should_panic]
    #[test]
    fn test_panic() {
        let point = FailPoint::new();
        point.set_actions("", vec![Action::new(Task::Panic(None), 1.0, None)]);
        point.eval("test_fail_point_panic");
    }

    #[test]
    fn test_print() {
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

        let point = FailPoint::new();
        point.set_actions("", vec![Action::new(Task::Print(None), 1.0, None)]);
        assert!(point.eval("test_fail_point_print").is_none());
        let msg = buffer.lock().unwrap().pop().unwrap();
        assert_eq!(msg, "failpoint test_fail_point_print executed.");
    }

    #[test]
    fn test_pause() {
        let point = Arc::new(FailPoint::new());
        point.set_actions("", vec![Action::new(Task::Pause, 1.0, None)]);
        let p = point.clone();
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            assert_eq!(p.eval("test_fail_point_pause"), None);
            tx.send(()).unwrap();
        });
        assert!(rx.recv_timeout(Duration::from_secs(1)).is_err());
        point.set_actions("", vec![Action::new(Task::Off, 1.0, None)]);
        rx.recv_timeout(Duration::from_secs(1)).unwrap();
    }

    #[tokio::test]
    async fn test_async_pause() {
        let point = Arc::new(FailPoint::new());
        point.set_actions("", vec![Action::new(Task::Pause, 1.0, None)]);
        let p = point.clone();
        let (tx, mut rx) = tokio::sync::mpsc::channel(2);
        let handle = tokio::spawn(async move {
            assert_eq!(p.eval_async("test_fail_point_pause").await, None);
            tx.send(()).await.unwrap()
        });
        assert!(rx.try_recv().is_err());
        point.set_actions("", vec![Action::new(Task::Off, 1.0, None)]);
        rx.recv().await.unwrap();
    }

    #[tokio::test(flavor="current_thread", start_paused=true)]
    async fn test_async_sleep() {
        let value = Arc::new(AtomicU64::new(0));

        fn spawn_sleep_task(
            sleep_duration_millis: u64,
            value: Arc<AtomicU64>,
            value_to_set: u64
        ) -> tokio::task::JoinHandle<()> {
            let point = Arc::new(FailPoint::new());
            point.set_actions("", vec![Action::new(
                Task::Sleep(sleep_duration_millis), 1.0, None)
            ]);
            let p = point.clone();
            tokio::spawn(async move {
                assert_eq!(p.eval_async("test_fail_point_sleep").await, None);
                value.store(value_to_set, Relaxed);
            })
        }

        let h1 = spawn_sleep_task(10, value.clone(), 10);
        let h2 = spawn_sleep_task(5, value.clone(), 5);

        tokio::join!(h2);
        assert_eq!(value.load(Relaxed), 5);
        tokio::join!(h1);
        assert_eq!(value.load(Relaxed), 10);
    }

    #[test]
    fn test_yield() {
        let point = FailPoint::new();
        point.set_actions("", vec![Action::new(Task::Yield, 1.0, None)]);
        assert!(point.eval("test_fail_point_yield").is_none());
    }

    #[test]
    fn test_delay() {
        let point = FailPoint::new();
        let timer = Instant::now();
        point.set_actions("", vec![Action::new(Task::Delay(1000), 1.0, None)]);
        assert!(point.eval("test_fail_point_delay").is_none());
        assert!(timer.elapsed() > Duration::from_millis(1000));
    }

    #[test]
    fn test_frequency_and_count() {
        let point = FailPoint::new();
        point.set_actions("", vec![Action::new(Task::Return(None), 0.8, Some(100))]);
        let mut count = 0;
        let mut times = 0f64;
        while count < 100 {
            if point.eval("test_fail_point_frequency").is_some() {
                count += 1;
            }
            times += 1f64;
        }
        assert!(100.0 / 0.9 < times && times < 100.0 / 0.7, "{}", times);
        for _ in 0..times as u64 {
            assert!(point.eval("test_fail_point_frequency").is_none());
        }
    }

    #[test]
    fn test_parse() {
        let cases = vec![
            ("return", Action::new(Task::Return(None), 1.0, None)),
            (
                "return(64)",
                Action::new(Task::Return(Some("64".to_owned())), 1.0, None),
            ),
            ("5*return", Action::new(Task::Return(None), 1.0, Some(5))),
            ("25%return", Action::new(Task::Return(None), 0.25, None)),
            (
                "125%2*return",
                Action::new(Task::Return(None), 1.25, Some(2)),
            ),
            (
                "return(2%5)",
                Action::new(Task::Return(Some("2%5".to_owned())), 1.0, None),
            ),
            ("125%2*off", Action::new(Task::Off, 1.25, Some(2))),
            (
                "125%2*sleep(100)",
                Action::new(Task::Sleep(100), 1.25, Some(2)),
            ),
            (" 125%2*off ", Action::new(Task::Off, 1.25, Some(2))),
            ("125%2*panic", Action::new(Task::Panic(None), 1.25, Some(2))),
            (
                "125%2*panic(msg)",
                Action::new(Task::Panic(Some("msg".to_owned())), 1.25, Some(2)),
            ),
            ("125%2*print", Action::new(Task::Print(None), 1.25, Some(2))),
            (
                "125%2*print(msg)",
                Action::new(Task::Print(Some("msg".to_owned())), 1.25, Some(2)),
            ),
            ("125%2*pause", Action::new(Task::Pause, 1.25, Some(2))),
            ("125%2*yield", Action::new(Task::Yield, 1.25, Some(2))),
            ("125%2*delay(2)", Action::new(Task::Delay(2), 1.25, Some(2))),
        ];
        for (expr, exp) in cases {
            let res: Action = expr.parse().unwrap();
            assert_eq!(res, exp);
        }

        let fail_cases = vec![
            "delay",
            "sleep",
            "Return",
            "ab%return",
            "ab*return",
            "return(msg",
            "unknown",
        ];
        for case in fail_cases {
            assert!(case.parse::<Action>().is_err());
        }
    }

    // This case should be tested as integration case, but when calling `teardown` other cases
    // like `test_pause` maybe also affected, so it's better keep it here.
    #[test]
    #[cfg_attr(not(feature = "failpoints"), ignore)]
    fn test_setup_and_teardown() {
        let fp_registry = Arc::new(FailPointRegistry::default());
        let f1 = || {
            fail_point!(fp_registry.clone(), "setup_and_teardown1", |_| 1);
            0
        };
        let fp_registry_clone = fp_registry.clone();
        let f2 = || {
            fail_point!(fp_registry_clone, "setup_and_teardown2", |_| 2);
            0
        };
        /*env::set_var(
            "FAILPOINTS",
            "setup_and_teardown1=return;setup_and_teardown2=pause;",
        );*/
        cfg(fp_registry.clone(), "setup_and_teardown1", "return");
        cfg(fp_registry.clone(), "setup_and_teardown2", "pause");
        assert_eq!(f1(), 1);

        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            tx.send(f2()).unwrap();
        });
        assert!(rx.recv_timeout(Duration::from_millis(500)).is_err());

        cfg(fp_registry.clone(), "setup_and_teardown1", "off");
        cfg(fp_registry.clone(), "setup_and_teardown2", "off");

        assert_eq!(rx.recv_timeout(Duration::from_millis(500)).unwrap(), 0);
        assert_eq!(f1(), 0);
    }
}
