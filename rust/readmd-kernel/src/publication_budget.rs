//! One operation budget across resource probes and nested publication engines.
//! A deadline is thread-bound; resource callbacks capture cancellation explicitly.
use serde_json::Value;
use std::{
    cell::Cell,
    sync::Arc,
    time::{Duration, Instant},
};

#[derive(Clone, Copy)]
struct Deadline {
    at: Instant,
    code: &'static str,
}
thread_local! { static CURRENT:Cell<Option<Deadline>>=const{Cell::new(None)}; }

pub(crate) struct Check {
    deadline: Option<Deadline>,
    cancel: Arc<dyn Fn() -> bool + Send + Sync>,
}
impl Check {
    pub(crate) fn check(&self) -> Result<(), String> {
        if (self.cancel)() {
            return Err("cancelled".into());
        }
        if let Some(deadline) = self.deadline.filter(|value| Instant::now() >= value.at) {
            return Err(deadline.code.into());
        }
        Ok(())
    }
    pub(crate) fn api(&self) -> crate::ApiResult<()> {
        self.check().map_err(|code| crate::ApiError::new(422, code))
    }
    /// Resource absence may select a fallback. Interruption must never do so.
    pub(crate) fn interrupted(&self, code: &str) -> bool {
        code == "cancelled" || code == "typst_timeout" || code == "latex_timeout"
    }
}
pub(crate) fn capture() -> Check {
    Check {
        deadline: CURRENT.with(Cell::get),
        cancel: crate::speech::cancellation_check(),
    }
}
pub(crate) fn check() -> Result<(), String> {
    capture().check()
}
pub(crate) fn deadline(timeout: Duration) -> Instant {
    let local = Instant::now() + timeout;
    CURRENT.with(|value| value.get().map_or(local, |outer| outer.at.min(local)))
}
fn within<T>(
    at: Instant,
    code: &'static str,
    action: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    struct Restore(Option<Deadline>);
    impl Drop for Restore {
        fn drop(&mut self) {
            CURRENT.with(|value| value.set(self.0));
        }
    }
    let previous = CURRENT.with(Cell::get);
    let selected = previous
        .filter(|outer| outer.at <= at)
        .unwrap_or(Deadline { at, code });
    CURRENT.with(|value| value.set(Some(selected)));
    let _restore = Restore(previous);
    check()?;
    // Writers check before their commit. Do not turn a successfully committed
    // result into a timeout after the fact.
    action()
}
pub(crate) fn run<T>(
    format: &str,
    options: &Value,
    action: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    let code = if format.eq_ignore_ascii_case("pdf")
        && options.pointer("/pdf/engine").and_then(Value::as_str) == Some("typst")
    {
        "typst_timeout"
    } else {
        "latex_timeout"
    };
    within(Instant::now() + Duration::from_secs(120), code, action)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nested_operations_keep_the_original_deadline_and_restore_context() {
        let at = Instant::now() + Duration::from_secs(1);
        assert!(CURRENT.with(Cell::get).is_none());
        within(at, "typst_timeout", || {
            run(
                "pdf",
                &serde_json::json!({"pdf":{"engine":"typst"}}),
                || {
                    assert_eq!(deadline(Duration::from_secs(120)), at);
                    Ok(())
                },
            )?;
            assert_eq!(deadline(Duration::from_secs(120)), at);
            Ok(())
        })
        .unwrap();
        assert!(CURRENT.with(Cell::get).is_none());
    }
    #[test]
    fn elapsed_outer_budget_cannot_be_reset_by_an_inner_operation() {
        let result: Result<(), String> = within(
            Instant::now() - Duration::from_millis(1),
            "typst_timeout",
            || panic!("expired operation ran"),
        );
        assert_eq!(result.unwrap_err(), "typst_timeout");
        assert!(CURRENT.with(Cell::get).is_none());
    }
    #[test]
    fn captured_checks_keep_cancellation_and_deadline_on_another_thread() {
        let flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let worker_flag = flag.clone();
        crate::speech::with_cancel(
            Arc::new(move || worker_flag.load(std::sync::atomic::Ordering::Acquire)),
            || {
                within(
                    Instant::now() + Duration::from_secs(1),
                    "typst_timeout",
                    || {
                        let captured = capture();
                        flag.store(true, std::sync::atomic::Ordering::Release);
                        assert_eq!(
                            std::thread::spawn(move || captured.check())
                                .join()
                                .unwrap()
                                .unwrap_err(),
                            "cancelled"
                        );
                        Ok(())
                    },
                )
                .unwrap();
            },
        );
        let captured = within(
            Instant::now() + Duration::from_secs(1),
            "typst_timeout",
            || Ok(capture()),
        )
        .unwrap();
        let expired = Check {
            deadline: Some(Deadline {
                at: Instant::now() - Duration::from_millis(1),
                code: "typst_timeout",
            }),
            cancel: captured.cancel,
        };
        assert_eq!(
            std::thread::spawn(move || expired.api())
                .join()
                .unwrap()
                .unwrap_err()
                .code,
            "typst_timeout"
        );
    }
    #[test]
    fn panic_restores_the_previous_thread_budget() {
        let result = std::panic::catch_unwind(|| {
            within::<()>(
                Instant::now() + Duration::from_secs(1),
                "latex_timeout",
                || panic!("controlled"),
            )
        });
        assert!(result.is_err());
        assert!(CURRENT.with(Cell::get).is_none());
    }
    #[test]
    fn real_resource_mutex_wait_observes_captured_cancellation_without_creating_a_profile() {
        let data = tempfile::tempdir().unwrap();
        let data_path = data.path().to_owned();
        let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let cancel = flag.clone();
        let (ready, waiting) = std::sync::mpsc::sync_channel(1);
        let guard = crate::capability_packs::resource_guard();
        let worker = std::thread::spawn(move || {
            crate::speech::with_cancel(
                Arc::new(move || cancel.load(std::sync::atomic::Ordering::Acquire)),
                || {
                    within(
                        Instant::now() + Duration::from_secs(3),
                        "typst_timeout",
                        || {
                            let budget = capture();
                            let count = std::sync::atomic::AtomicUsize::new(0);
                            crate::publication_fonts::scope_checked(&data_path, &assets, &|| {
                                // Entry + first lock attempt + second lock attempt: a real
                                // WouldBlock iteration has happened while our guard is held.
                                if count.fetch_add(1, std::sync::atomic::Ordering::AcqRel) == 2 {
                                    ready.send(()).unwrap();
                                }
                                budget.api()
                            })
                            .map(|_| ())
                            .map_err(|error| error.code)
                        },
                    )
                },
            )
        });
        waiting.recv_timeout(Duration::from_secs(2)).unwrap();
        flag.store(true, std::sync::atomic::Ordering::Release);
        assert_eq!(worker.join().unwrap().unwrap_err(), "cancelled");
        drop(guard);
        assert_eq!(std::fs::read_dir(data.path()).unwrap().count(), 0);
    }
    #[test]
    fn key_cancellation_is_not_downgraded_to_a_reusable_fallback() {
        let data = tempfile::tempdir().unwrap();
        let assets = tempfile::tempdir().unwrap();
        crate::speech::with_cancel(Arc::new(|| true), || {
            let options = serde_json::json!({"pdf":{"engine":"typst"}});
            assert_eq!(
                crate::export_preview::key_with_data_checked(
                    "pdf",
                    "text",
                    "",
                    &options,
                    "readmd",
                    data.path(),
                    assets.path()
                )
                .unwrap_err(),
                "cancelled"
            );
            let a = crate::export_preview::key_with_data(
                "pdf",
                "text",
                "",
                &options,
                "readmd",
                data.path(),
                assets.path(),
            );
            let b = crate::export_preview::key_with_data(
                "pdf",
                "text",
                "",
                &options,
                "readmd",
                data.path(),
                assets.path(),
            );
            assert!(a.starts_with("uncached:") && b.starts_with("uncached:"));
            assert_ne!(a, b);
        });
        assert_eq!(std::fs::read_dir(data.path()).unwrap().count(), 0);
    }
}
