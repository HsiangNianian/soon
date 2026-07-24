use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn isolated_home() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock")
        .as_nanos();
    let path =
        std::env::temp_dir().join(format!("soon-events-test-{}-{nonce}", std::process::id()));
    std::fs::create_dir_all(&path).expect("create isolated home");
    path
}

fn soon(home: &PathBuf) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_soon"));
    command
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("XDG_DATA_HOME", home.join(".local/share"));
    command
}

fn record_command(
    home: &PathBuf,
    id: &str,
    command: &str,
    exit_code: &str,
    previous_id: Option<&str>,
) {
    let mut args = vec![
        "events",
        "record-command",
        "--id",
        id,
        "--command",
        command,
        "--cwd",
        "/tmp/soon-project",
        "--started-at-ms",
        "1000",
        "--duration-ms",
        "25",
        "--exit-code",
        exit_code,
        "--shell",
        "zsh",
    ];
    if let Some(previous_id) = previous_id {
        args.extend(["--previous-id", previous_id]);
    }

    let output = soon(home)
        .args(args)
        .output()
        .expect("record command event");
    assert!(
        output.status.success(),
        "record failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn recorded_command_is_counted_without_leaking_its_text() {
    let home = isolated_home();
    let secret_marker = "cargo test --workspace --token should-not-be-printed";

    let record = soon(&home)
        .args([
            "events",
            "record-command",
            "--id",
            "command-1",
            "--command",
            secret_marker,
            "--cwd",
            "/tmp/soon-project",
            "--started-at-ms",
            "1000",
            "--duration-ms",
            "25",
            "--exit-code",
            "0",
            "--shell",
            "zsh",
        ])
        .output()
        .expect("record command event");

    assert!(
        record.status.success(),
        "record failed: {}",
        String::from_utf8_lossy(&record.stderr)
    );
    assert!(record.stdout.is_empty(), "record command wrote to stdout");

    let inspect = soon(&home)
        .args(["events", "inspect"])
        .output()
        .expect("inspect event store");
    let stdout = String::from_utf8(inspect.stdout).expect("UTF-8 inspect output");

    let _ = std::fs::remove_dir_all(&home);
    assert!(
        inspect.status.success(),
        "inspect failed: {}",
        String::from_utf8_lossy(&inspect.stderr)
    );
    assert!(stdout.contains("Schema version: 1"), "{stdout}");
    assert!(stdout.contains("Command events: 1"), "{stdout}");
    assert!(stdout.contains("Suggestion events: 0"), "{stdout}");
    assert!(
        !stdout.contains(secret_marker),
        "inspect leaked command text"
    );
}

#[test]
fn command_result_selects_repair_or_next_step_policy() {
    let home = isolated_home();
    std::fs::write(
        home.join(".zsh_history"),
        "cargo test\necho history-fallback\ncargo test\necho history-fallback\n",
    )
    .expect("write Zsh history");

    record_command(&home, "failed-1", "cargo test", "1", None);
    record_command(
        &home,
        "repair-1",
        "cargo test -- --nocapture",
        "0",
        Some("failed-1"),
    );
    record_command(&home, "success-1", "cargo test", "0", Some("repair-1"));
    record_command(
        &home,
        "next-1",
        "git push --force-with-lease",
        "0",
        Some("success-1"),
    );

    let repair = soon(&home)
        .args([
            "--shell",
            "zsh",
            "now",
            "--raw",
            "--after",
            "cargo test",
            "--exit-code",
            "1",
            "--cwd",
            "/tmp/soon-project",
        ])
        .output()
        .expect("predict repair command");
    let next_step = soon(&home)
        .args([
            "--shell",
            "zsh",
            "now",
            "--raw",
            "--after",
            "cargo test",
            "--exit-code",
            "0",
            "--cwd",
            "/tmp/soon-project",
        ])
        .output()
        .expect("predict next-step command");

    let _ = std::fs::remove_dir_all(&home);
    assert!(repair.status.success(), "repair prediction failed");
    assert!(next_step.status.success(), "next-step prediction failed");
    assert_eq!(
        String::from_utf8(repair.stdout).expect("UTF-8 repair"),
        "cargo test -- --nocapture\n"
    );
    assert_eq!(
        String::from_utf8(next_step.stdout).expect("UTF-8 next step"),
        "git push --force-with-lease\n"
    );
}

#[test]
fn clearing_events_requires_confirmation_and_empties_the_store() {
    let home = isolated_home();
    record_command(&home, "command-1", "cargo test", "0", None);

    let refused = soon(&home)
        .args(["events", "clear"])
        .output()
        .expect("refuse unconfirmed clear");
    assert!(!refused.status.success(), "unconfirmed clear succeeded");

    let cleared = soon(&home)
        .args(["events", "clear", "--yes"])
        .output()
        .expect("clear event store");
    assert!(
        cleared.status.success(),
        "confirmed clear failed: {}",
        String::from_utf8_lossy(&cleared.stderr)
    );

    let inspect = soon(&home)
        .args(["events", "inspect"])
        .output()
        .expect("inspect cleared event store");
    let stdout = String::from_utf8(inspect.stdout).expect("UTF-8 inspect output");

    let _ = std::fs::remove_dir_all(&home);
    assert!(stdout.contains("Command events: 0"), "{stdout}");
    assert!(stdout.contains("Suggestion events: 0"), "{stdout}");
}

#[test]
fn suggestion_feedback_outcomes_are_counted_without_leaking_candidates() {
    let home = isolated_home();
    let candidate = "deploy --token should-not-be-printed";

    for outcome in ["shown", "accepted", "executed", "dismissed"] {
        let output = soon(&home)
            .args([
                "events",
                "record-suggestion",
                "--id",
                "suggestion-1",
                "--command-event-id",
                "command-1",
                "--trigger",
                "repair",
                "--candidate-source",
                "history",
                "--command",
                candidate,
                "--outcome",
                outcome,
                "--latency-ms",
                "12.5",
            ])
            .output()
            .expect("record suggestion event");
        assert!(
            output.status.success(),
            "record {outcome} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let inspect = soon(&home)
        .args(["events", "inspect"])
        .output()
        .expect("inspect suggestion events");
    let stdout = String::from_utf8(inspect.stdout).expect("UTF-8 inspect output");

    let _ = std::fs::remove_dir_all(&home);
    assert!(stdout.contains("Suggestion events: 4"), "{stdout}");
    assert!(stdout.contains("Shown: 1"), "{stdout}");
    assert!(stdout.contains("Accepted: 1"), "{stdout}");
    assert!(stdout.contains("Executed: 1"), "{stdout}");
    assert!(stdout.contains("Dismissed: 1"), "{stdout}");
    assert!(!stdout.contains(candidate), "inspect leaked candidate text");
}

#[test]
fn duplicate_command_event_ids_are_idempotent() {
    let home = isolated_home();
    record_command(&home, "same-command", "cargo test", "0", None);
    record_command(&home, "same-command", "cargo test", "0", None);

    let inspect = soon(&home)
        .args(["events", "inspect"])
        .output()
        .expect("inspect deduplicated events");
    let stdout = String::from_utf8(inspect.stdout).expect("UTF-8 inspect output");

    let _ = std::fs::remove_dir_all(&home);
    assert!(stdout.contains("Command events: 1"), "{stdout}");
}

#[test]
fn configured_retention_keeps_only_the_newest_events() {
    let home = isolated_home();
    let configure = soon(&home)
        .args(["config", "set", "events.retention", "3"])
        .output()
        .expect("configure event retention");
    assert!(
        configure.status.success(),
        "configure failed: {}",
        String::from_utf8_lossy(&configure.stderr)
    );

    record_command(&home, "command-1", "echo one", "0", None);
    record_command(&home, "command-2", "echo two", "0", Some("command-1"));
    record_command(&home, "command-3", "echo three", "0", Some("command-2"));
    record_command(&home, "command-4", "echo four", "0", Some("command-3"));

    let inspect = soon(&home)
        .args(["events", "inspect"])
        .output()
        .expect("inspect retained events");
    let stdout = String::from_utf8(inspect.stdout).expect("UTF-8 inspect output");

    let _ = std::fs::remove_dir_all(&home);
    assert!(stdout.contains("Retention: last 3 events"), "{stdout}");
    assert!(stdout.contains("Command events: 3"), "{stdout}");
}

#[test]
fn a_truncated_final_line_does_not_corrupt_the_next_event() {
    let home = isolated_home();
    let initial = soon(&home)
        .args(["events", "inspect"])
        .output()
        .expect("inspect event path");
    let initial_stdout = String::from_utf8(initial.stdout).expect("UTF-8 inspect output");
    let event_path = initial_stdout
        .lines()
        .find_map(|line| line.strip_prefix("Event store: "))
        .map(PathBuf::from)
        .expect("event store path");
    std::fs::create_dir_all(event_path.parent().expect("event parent"))
        .expect("create event directory");
    std::fs::write(&event_path, b"{\"truncated\":").expect("write truncated final event line");

    record_command(&home, "command-1", "cargo test", "0", None);
    let inspect = soon(&home)
        .args(["events", "inspect"])
        .output()
        .expect("inspect recovered event store");
    let stdout = String::from_utf8(inspect.stdout).expect("UTF-8 inspect output");

    let _ = std::fs::remove_dir_all(&home);
    assert!(stdout.contains("Command events: 1"), "{stdout}");
    assert!(stdout.contains("Malformed lines: 1"), "{stdout}");
}
