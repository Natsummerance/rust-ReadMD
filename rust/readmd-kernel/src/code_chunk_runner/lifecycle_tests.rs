use super::*;
use std::sync::atomic::{AtomicBool, Ordering};

const LONG_SQL: &str = "WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<3000000) SELECT sum(x) FROM n;";

#[test]
fn already_cancelled_request_does_not_create_a_script_or_run_sql() {
    let result = crate::speech::with_cancel(Arc::new(|| true), || {
        execute_code_chunk("SELECT 42;", "sql", false, 10, None)
    });
    assert_eq!(get_str(&result, "error_code"), "execution_cancelled");
    assert_eq!(get_str(&result, "stdout"), "");
}

#[test]
fn already_cancelled_sql_has_no_successful_result() {
    let result = crate::speech::with_cancel(Arc::new(|| true), || {
        execute_sql_chunk("SELECT 42;", None, 10)
    });
    assert_eq!(get_str(&result, "error_code"), "execution_cancelled");
    assert!(!is_truthy(&result));
    assert_eq!(get_str(&result, "stdout"), "");
}

#[test]
fn in_memory_sql_cannot_create_attached_or_vacuum_files() {
    let root = tempfile::tempdir().unwrap();
    for (name, sql) in [
        (
            "attached.sqlite",
            "ATTACH DATABASE '{}' AS other; CREATE TABLE other.data(x);",
        ),
        ("vacuum.sqlite", "VACUUM INTO '{}';"),
    ] {
        let target = root.path().join(name);
        let source = sql.replace("{}", &target.to_string_lossy().replace('\'', "''"));
        let result = execute_sql_chunk(&source, None, 1);
        assert!(
            !target.exists(),
            "in-memory execution wrote a database outside its owned connection"
        );
        assert!(!is_truthy(&result));
    }
}

#[test]
fn running_sql_observes_cancel_instead_of_returning_late_sum() {
    let cancel = Arc::new(AtomicBool::new(false));
    let sent = Arc::new(Mutex::new(None));
    std::thread::scope(|scope| {
        let flag = cancel.clone();
        let at = sent.clone();
        scope.spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            *at.lock().unwrap() = Some(Instant::now());
            flag.store(true, Ordering::Release);
        });
        let started = Instant::now();
        let result =
            crate::speech::with_cancel(Arc::new(move || cancel.load(Ordering::Acquire)), || {
                execute_sql_chunk(LONG_SQL, None, 10)
            });
        println!("SQL cancel total_ms={}", started.elapsed().as_millis());
        assert_eq!(get_str(&result, "error_code"), "execution_cancelled");
        assert!(
            sent.lock().unwrap().unwrap().elapsed() < Duration::from_millis(200),
            "cancel observation exceeded 200ms"
        );
        assert_eq!(get_str(&result, "stdout"), "");
    });
}

#[test]
fn running_process_observes_cancel_and_reaps_owned_child() {
    let cancel = Arc::new(AtomicBool::new(false));
    let sent = Arc::new(Mutex::new(None));
    std::thread::scope(|scope| {
        let flag = cancel.clone();
        let at = sent.clone();
        scope.spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            *at.lock().unwrap() = Some(Instant::now());
            flag.store(true, Ordering::Release);
        });
        let exe = std::env::current_exe().unwrap().into_os_string();
        let started = Instant::now();
        let result =
            crate::speech::with_cancel(Arc::new(move || cancel.load(Ordering::Acquire)), || {
                run_process(
                    &[
                        exe,
                        os("--exact"),
                        os("code_chunk_runner::lifecycle_tests::owned_waiting_child"),
                        os("--ignored"),
                        os("--nocapture"),
                    ],
                    None,
                    10,
                    None,
                )
            });
        println!("Process cancel total_ms={}", started.elapsed().as_millis());
        assert_eq!(get_str(&result, "error_code"), "execution_cancelled");
        assert!(
            sent.lock().unwrap().unwrap().elapsed() < Duration::from_millis(200),
            "cancel observation exceeded 200ms"
        );
        assert!(!get_str(&result, "stdout").contains("LATE"));
    });
}

#[test]
fn sql_deadline_stops_the_actual_query_and_next_request_runs() {
    let started = Instant::now();
    let result = execute_sql_chunk("WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<100000000) SELECT sum(x) FROM n;", None, 1);
    assert_eq!(get_str(&result, "error_code"), "execution_timeout");
    assert!(started.elapsed() >= Duration::from_secs(1));
    assert!(started.elapsed() < Duration::from_secs(2));
    assert_eq!(get_str(&result, "stdout"), "");
    let next = execute_sql_chunk("SELECT 42 AS answer;", None, 1);
    assert!(is_truthy(&next));
    assert!(get_str(&next, "stdout").contains("42"));
}

#[test]
fn completed_root_cannot_leave_a_descendant_holding_the_pipes() {
    let started = Instant::now();
    let result = run_process(
        &[
            std::env::current_exe().unwrap().into_os_string(),
            os("--exact"),
            os("code_chunk_runner::lifecycle_tests::owned_descendant_root"),
            os("--ignored"),
            os("--nocapture"),
        ],
        None,
        10,
        None,
    );
    assert!(is_truthy(&result), "{result:?}");
    assert!(get_str(&result, "stdout").contains("ROOT_READY"));
    assert!(!get_str(&result, "stdout").contains("LATE"));
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "descendant retained the pipe after root exit"
    );
}

#[test]
#[ignore = "bounded descendant consumer invoked only by the lifecycle test"]
fn owned_descendant_root() {
    let _child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "code_chunk_runner::lifecycle_tests::owned_waiting_child",
            "--ignored",
            "--nocapture",
        ])
        .spawn()
        .unwrap();
    println!("ROOT_READY");
}

#[test]
#[ignore = "bounded child consumer invoked only by the lifecycle test"]
fn owned_waiting_child() {
    std::thread::sleep(Duration::from_secs(2));
    println!("LATE");
}
