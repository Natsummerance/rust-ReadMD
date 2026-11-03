//! Process-owned code admission and graceful exit, independent of caller task IDs.
//! This never inspects or signals ambient processes or model services.
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Condvar, Mutex, OnceLock,
    },
    time::{Duration, Instant},
};

#[derive(Default)]
struct State {
    closing: bool,
    next: u64,
    active: HashMap<u64, Arc<AtomicBool>>,
}
#[derive(Default)]
struct Registry {
    state: Mutex<State>,
    idle: Condvar,
}
struct Lease<'a> {
    registry: &'a Registry,
    id: u64,
    cancelled: Arc<AtomicBool>,
}
impl Drop for Lease<'_> {
    fn drop(&mut self) {
        let mut state = self
            .registry
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        state.active.remove(&self.id);
        self.registry.idle.notify_all();
    }
}
impl Registry {
    fn run<T>(&self, action: impl FnOnce() -> T) -> Option<T> {
        let lease = {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            if state.closing {
                return None;
            }
            let id = state.next.checked_add(1)?;
            state.next = id;
            let cancelled = Arc::new(AtomicBool::new(false));
            state.active.insert(id, cancelled.clone());
            Lease {
                registry: self,
                id,
                cancelled,
            }
        };
        let outer = crate::speech::cancellation_check();
        let cancelled = lease.cancelled.clone();
        Some(crate::speech::with_cancel(
            Arc::new(move || outer() || cancelled.load(Ordering::Acquire)),
            action,
        ))
    }
    fn stop(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout.min(Duration::from_secs(2));
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.closing = true;
        for flag in state.active.values() {
            flag.store(true, Ordering::Release);
        }
        while !state.active.is_empty() {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return false;
            }
            let (next, _) = self
                .idle
                .wait_timeout(state, remaining)
                .unwrap_or_else(|e| e.into_inner());
            state = next;
        }
        true
    }
}
fn registry() -> &'static Registry {
    static REGISTRY: OnceLock<Registry> = OnceLock::new();
    REGISTRY.get_or_init(Registry::default)
}
pub(super) fn run<T>(action: impl FnOnce() -> T) -> Option<T> {
    registry().run(action)
}
pub(super) fn stop(timeout: Duration) -> bool {
    registry().stop(timeout)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn graceful_stop_refuses_late_admission() {
        let registry = Registry::default();
        assert_eq!(registry.run(|| 42), Some(42));
        assert!(registry.stop(Duration::ZERO));
        assert_eq!(
            registry.run(|| panic!("must not execute after shutdown")),
            None::<()>
        );
    }

    #[test]
    fn stop_waits_for_actual_owner_cleanup_and_times_out_honestly() {
        let registry = Registry::default();
        let (started, ready) = mpsc::channel();
        let (finish, cleanup) = mpsc::channel();
        std::thread::scope(|scope| {
            let owner = &registry;
            let task = scope.spawn(move || {
                owner.run(|| {
                    started.send(()).unwrap();
                    while !crate::speech::cancelled() {
                        std::thread::sleep(Duration::from_millis(1));
                    }
                    cleanup.recv().unwrap();
                })
            });
            ready.recv().unwrap();
            assert!(
                !registry.stop(Duration::from_millis(20)),
                "flagging cancellation is not cleanup completion"
            );
            assert_eq!(registry.run(|| 42), None);
            finish.send(()).unwrap();
            assert_eq!(task.join().unwrap(), Some(()));
            assert!(registry.stop(Duration::from_secs(2)));
        });
    }

    #[test]
    fn caller_cancellation_and_unwinding_keep_their_original_scope() {
        let registry = Registry::default();
        assert_eq!(
            crate::speech::with_cancel(Arc::new(|| true), || registry
                .run(crate::speech::cancelled)),
            Some(true)
        );
        assert_eq!(registry.run(crate::speech::cancelled), Some(false));
        assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(
            || registry.run(|| panic!("owned test panic"))
        ))
        .is_err());
        assert!(
            registry.stop(Duration::ZERO),
            "unwinding must release the exact lease"
        );
    }

    #[test]
    fn graceful_stop_interrupts_the_actual_sql_and_waits_for_return() {
        let registry = Registry::default();
        let (started, ready) = mpsc::channel();
        std::thread::scope(|scope| {
            let task = scope.spawn(|| registry.run(|| {
                started.send(()).unwrap();
                super::super::execute_sql_chunk("WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<100000000) SELECT sum(x) FROM n;", None, 10)
            }));
            ready.recv().unwrap();
            std::thread::sleep(Duration::from_millis(50));
            let at = Instant::now();
            assert!(registry.stop(Duration::from_secs(2)));
            assert!(at.elapsed() < Duration::from_millis(200));
            let result = task.join().unwrap().unwrap();
            assert_eq!(
                result.get("error_code").and_then(|v| v.as_str()),
                Some("execution_cancelled")
            );
            assert_eq!(result.get("stdout").and_then(|v| v.as_str()), Some(""));
        });
    }
}
