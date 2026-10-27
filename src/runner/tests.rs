use super::*;
use std::io::{Read, Write};

fn fixture(mode: &str) -> Command {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command.args([
        "--exact",
        "runner::tests::child_fixture",
        "--ignored",
        "--nocapture",
    ]);
    command.env("PERSONAL_DESK_RUNNER_TEST_MODE", mode);
    command
}

#[test]
#[ignore = "internal child-process fixture, invoked by runner regression tests"]
fn child_fixture() {
    let mode = std::env::var("PERSONAL_DESK_RUNNER_TEST_MODE").unwrap();
    fs::write("started", std::process::id().to_string()).unwrap();
    match mode.as_str() {
        "io" => {
            let mut input = String::new();
            std::io::stdin().read_to_string(&mut input).unwrap();
            println!("received:{input}");
            eprintln!("diagnostic");
        }
        "exit" => std::process::exit(7),
        "output" | "error" => {
            let bytes = vec![b'x'; 65_537];
            if mode == "output" {
                std::io::stdout().write_all(&bytes).unwrap();
            } else {
                std::io::stderr().write_all(&bytes).unwrap();
            }
        }
        "wait" => loop {
            std::thread::sleep(Duration::from_millis(10));
        },
        _ => panic!("unknown fixture mode"),
    }
}

#[test]
fn cancelled_and_zero_timeout_runs_do_not_spawn_or_truncate_outputs() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("stdout.txt"), "previous output").unwrap();
    for (cancel, timeout) in [(true, Duration::from_secs(5)), (false, Duration::ZERO)] {
        assert!(
            run(
                &mut fixture("io"),
                dir.path(),
                "",
                timeout,
                &AtomicBool::new(cancel)
            )
            .is_err()
        );
        assert!(!dir.path().join("started").exists());
        assert_eq!(
            fs::read_to_string(dir.path().join("stdout.txt")).unwrap(),
            "previous output"
        );
    }
}

#[test]
fn runner_preserves_unicode_input_and_separates_output_and_errors() {
    let dir = tempfile::tempdir().unwrap();
    let (ok, output, error) = run(
        &mut fixture("io"),
        dir.path(),
        "中文 café\n",
        Duration::from_secs(10),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert!(ok);
    assert!(output.contains("received:中文 café"));
    assert!(error.contains("diagnostic"));
}

#[test]
fn nonzero_exit_and_missing_program_are_distinct_from_success() {
    let dir = tempfile::tempdir().unwrap();
    assert!(
        !run(
            &mut fixture("exit"),
            dir.path(),
            "",
            Duration::from_secs(10),
            &AtomicBool::new(false)
        )
        .unwrap()
        .0
    );
    assert!(
        run(
            &mut Command::new(dir.path().join("missing-program")),
            dir.path(),
            "",
            Duration::from_secs(10),
            &AtomicBool::new(false)
        )
        .unwrap_err()
        .contains("无法启动")
    );
}

#[test]
fn fast_exits_cannot_bypass_output_limits_on_either_stream() {
    for mode in ["output", "error"] {
        let dir = tempfile::tempdir().unwrap();
        assert!(
            run(
                &mut fixture(mode),
                dir.path(),
                "",
                Duration::from_secs(10),
                &AtomicBool::new(false)
            )
            .unwrap_err()
            .contains("64 KiB")
        );
    }
}

#[test]
fn timeout_reaps_child_and_releases_its_files() {
    let dir = tempfile::tempdir().unwrap();
    assert!(
        run(
            &mut fixture("wait"),
            dir.path(),
            "",
            Duration::from_millis(150),
            &AtomicBool::new(false)
        )
        .unwrap_err()
        .contains("超时")
    );
    fs::remove_file(dir.path().join("stdout.txt")).unwrap();
    fs::remove_file(dir.path().join("stderr.txt")).unwrap();
}

#[test]
fn cancellation_after_start_reaps_the_running_child() {
    let dir = tempfile::tempdir().unwrap();
    let cancel = AtomicBool::new(false);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            let deadline = Instant::now() + Duration::from_secs(5);
            while !dir.path().join("started").exists() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(5));
            }
            cancel.store(true, Ordering::Relaxed);
        });
        assert!(
            run(
                &mut fixture("wait"),
                dir.path(),
                "",
                Duration::from_secs(10),
                &cancel
            )
            .unwrap_err()
            .contains("已停止")
        );
    });
    fs::remove_file(dir.path().join("stdout.txt")).unwrap();
}
