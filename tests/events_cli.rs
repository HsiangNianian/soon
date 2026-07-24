use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT_HOME: AtomicU64 = AtomicU64::new(0);

fn isolated_home() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock")
        .as_nanos();
    let sequence = NEXT_HOME.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "soon-events-test-{}-{nonce}-{sequence}",
        std::process::id()
    ));
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
    let secret_marker = "cargo test --workspace --label should-not-be-printed";

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
    let candidate = "deploy --label should-not-be-printed";

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

#[test]
fn sensitive_commands_are_rejected_before_persistence() {
    let home = isolated_home();
    let secret = "super-secret-value";
    let command = format!("curl -H 'Authorization: Bearer {secret}' https://example.test");

    let record = soon(&home)
        .args([
            "events",
            "record-command",
            "--id",
            "secret-command",
            "--command",
            &command,
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
        .expect("reject sensitive command event");

    assert!(!record.status.success(), "sensitive command was persisted");
    assert!(record.stdout.is_empty(), "rejection wrote to stdout");
    assert!(
        !String::from_utf8_lossy(&record.stderr).contains(secret),
        "rejection leaked the secret"
    );

    let inspect = soon(&home)
        .args(["events", "inspect"])
        .output()
        .expect("inspect event store");
    let stdout = String::from_utf8(inspect.stdout).expect("UTF-8 inspect output");

    let _ = std::fs::remove_dir_all(&home);
    assert!(stdout.contains("Command events: 0"), "{stdout}");
}

#[test]
fn sensitive_suggestions_are_rejected_before_persistence() {
    let home = isolated_home();
    let secret = "should-not-be-stored";
    let command = format!("deploy --token {secret}");

    let record = soon(&home)
        .args([
            "events",
            "record-suggestion",
            "--id",
            "secret-suggestion",
            "--trigger",
            "repair",
            "--candidate-source",
            "model",
            "--command",
            &command,
            "--outcome",
            "shown",
            "--latency-ms",
            "12.5",
        ])
        .output()
        .expect("reject sensitive suggestion event");

    assert!(
        !record.status.success(),
        "sensitive suggestion was persisted"
    );
    assert!(record.stdout.is_empty(), "rejection wrote to stdout");
    assert!(
        !String::from_utf8_lossy(&record.stderr).contains(secret),
        "rejection leaked the secret"
    );

    let inspect = soon(&home)
        .args(["events", "inspect"])
        .output()
        .expect("inspect event store");
    let stdout = String::from_utf8(inspect.stdout).expect("UTF-8 inspect output");

    let _ = std::fs::remove_dir_all(&home);
    assert!(stdout.contains("Suggestion events: 0"), "{stdout}");
}

#[test]
fn previously_stored_sensitive_events_are_not_suggested() {
    let home = isolated_home();
    let secret = "legacy-secret-value";
    std::fs::write(
        home.join(".zsh_history"),
        "cargo test\necho safe-fallback\ncargo test\necho safe-fallback\n",
    )
    .expect("write Zsh history");

    let inspect = soon(&home)
        .args(["events", "inspect"])
        .output()
        .expect("inspect event path");
    let inspect_stdout = String::from_utf8(inspect.stdout).expect("UTF-8 inspect output");
    let event_path = inspect_stdout
        .lines()
        .find_map(|line| line.strip_prefix("Event store: "))
        .map(PathBuf::from)
        .expect("event store path");
    std::fs::create_dir_all(event_path.parent().expect("event parent"))
        .expect("create event directory");

    let first = serde_json::json!({
        "kind": "command",
        "schema_version": 1,
        "id": "legacy-command",
        "command": "cargo test",
        "cwd": "/tmp/soon-project",
        "started_at_ms": 1000,
        "duration_ms": 25,
        "exit_code": 0,
        "shell": "zsh",
        "previous_event_id": null
    });
    let second = serde_json::json!({
        "kind": "command",
        "schema_version": 1,
        "id": "legacy-sensitive-successor",
        "command": format!("deploy --token {secret}"),
        "cwd": "/tmp/soon-project",
        "started_at_ms": 2000,
        "duration_ms": 25,
        "exit_code": 0,
        "shell": "zsh",
        "previous_event_id": "legacy-command"
    });
    std::fs::write(&event_path, format!("{first}\n{second}\n")).expect("seed legacy event store");

    let prediction = soon(&home)
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
        .expect("predict from legacy events");
    let stdout = String::from_utf8(prediction.stdout).expect("UTF-8 prediction");

    let _ = std::fs::remove_dir_all(&home);
    assert!(
        prediction.status.success(),
        "prediction failed: {}",
        String::from_utf8_lossy(&prediction.stderr)
    );
    assert_eq!(stdout, "echo safe-fallback\n");
    assert!(!stdout.contains(secret), "prediction leaked a secret");
}

#[test]
fn configured_literal_exclusions_reject_commands_without_echoing_values() {
    let home = isolated_home();
    let excluded = "internal-deploy --production";

    let configure = soon(&home)
        .args(["config", "set", "privacy.excluded_literals", excluded])
        .output()
        .expect("configure literal exclusion");
    assert!(
        configure.status.success(),
        "configure failed: {}",
        String::from_utf8_lossy(&configure.stderr)
    );
    assert!(
        !String::from_utf8_lossy(&configure.stdout).contains(excluded),
        "configuration output leaked excluded literal"
    );

    let record = soon(&home)
        .args([
            "events",
            "record-command",
            "--id",
            "excluded-command",
            "--command",
            &format!("{excluded} --region local"),
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
        .expect("reject configured exclusion");
    assert!(!record.status.success(), "excluded command was persisted");
    assert!(
        !String::from_utf8_lossy(&record.stderr).contains(excluded),
        "rejection leaked excluded literal"
    );

    let show = soon(&home)
        .arg("config")
        .output()
        .expect("show redacted config");
    let stdout = String::from_utf8(show.stdout).expect("UTF-8 config output");

    let _ = std::fs::remove_dir_all(&home);
    assert!(show.status.success(), "config display failed");
    assert!(!stdout.contains(excluded), "config display leaked literal");
    assert!(
        stdout.contains("excluded_literals = [\"<redacted>\"]"),
        "{stdout}"
    );
}

#[test]
fn configured_pattern_exclusions_are_validated_and_enforced() {
    let home = isolated_home();
    let pattern = r"(?i)^kubectl .*--context production";

    let configure = soon(&home)
        .args(["config", "set", "privacy.excluded_patterns", pattern])
        .output()
        .expect("configure pattern exclusion");
    assert!(
        configure.status.success(),
        "configure failed: {}",
        String::from_utf8_lossy(&configure.stderr)
    );

    let record = soon(&home)
        .args([
            "events",
            "record-command",
            "--id",
            "excluded-pattern-command",
            "--command",
            "kubectl get pods --context production",
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
        .expect("reject pattern exclusion");
    assert!(!record.status.success(), "pattern exclusion was ignored");

    let invalid = soon(&home)
        .args(["config", "set", "privacy.excluded_patterns", "["])
        .output()
        .expect("validate pattern exclusion");

    let _ = std::fs::remove_dir_all(&home);
    assert!(!invalid.status.success(), "invalid regex was accepted");
    assert!(
        String::from_utf8_lossy(&invalid.stderr).contains("Invalid privacy exclusion pattern"),
        "{}",
        String::from_utf8_lossy(&invalid.stderr)
    );
}

#[test]
fn provider_credentials_use_an_environment_variable_name() {
    let home = isolated_home();
    let secret = "legacy-config-secret";

    let legacy = soon(&home)
        .args(["config", "set", "llm.api_key", secret])
        .output()
        .expect("reject legacy credential storage");
    assert!(!legacy.status.success(), "stored a provider credential");
    let legacy_stderr = String::from_utf8_lossy(&legacy.stderr);
    assert!(!legacy_stderr.contains(secret), "error leaked credential");
    assert!(legacy_stderr.contains("llm.api_key_env"), "{legacy_stderr}");

    let configure = soon(&home)
        .args(["config", "set", "llm.api_key_env", "TEAM_PROVIDER_TOKEN"])
        .output()
        .expect("configure provider credential environment variable");

    let _ = std::fs::remove_dir_all(&home);
    assert!(
        configure.status.success(),
        "environment variable configuration failed: {}",
        String::from_utf8_lossy(&configure.stderr)
    );
}

#[test]
fn learn_statistics_do_not_render_sensitive_legacy_entries() {
    let home = isolated_home();
    let secret = "legacy-learn-secret";
    let config_path = soon(&home)
        .args(["config", "path"])
        .output()
        .expect("get config path");
    let learn_path = PathBuf::from(
        String::from_utf8(config_path.stdout)
            .expect("UTF-8 config path")
            .trim(),
    )
    .parent()
    .expect("config parent")
    .join("learn.json");
    std::fs::create_dir_all(learn_path.parent().expect("learn parent"))
        .expect("create learn directory");
    let learn_db = serde_json::json!({
        "transitions": {
            "git": {
                format!("deploy --token {secret}"): 3,
                "cargo test --workspace": 1
            }
        },
        "total_samples": 4
    });
    std::fs::write(&learn_path, learn_db.to_string()).expect("seed legacy learn database");

    let stats = soon(&home)
        .args(["learn", "stats"])
        .output()
        .expect("show learn statistics");
    let stdout = String::from_utf8(stats.stdout).expect("UTF-8 learn statistics");

    let _ = std::fs::remove_dir_all(&home);
    assert!(
        stats.status.success(),
        "learn stats failed: {}",
        String::from_utf8_lossy(&stats.stderr)
    );
    assert!(!stdout.contains(secret), "learn stats leaked a secret");
    assert!(!stdout.contains("deploy --token"), "{stdout}");
    assert!(stdout.contains("cargo test --workspace"), "{stdout}");
}

#[test]
fn similar_search_does_not_render_sensitive_legacy_entries() {
    let home = isolated_home();
    let secret = "legacy-similar-secret";
    let config_path = soon(&home)
        .args(["config", "path"])
        .output()
        .expect("get config path");
    let learn_path = PathBuf::from(
        String::from_utf8(config_path.stdout)
            .expect("UTF-8 config path")
            .trim(),
    )
    .parent()
    .expect("config parent")
    .join("learn.json");
    std::fs::create_dir_all(learn_path.parent().expect("learn parent"))
        .expect("create learn directory");
    let learn_db = serde_json::json!({
        "trigram_index": {
            format!("deploy --token {secret}"): {"$de": 1.0}
        },
        "total_samples": 1
    });
    std::fs::write(&learn_path, learn_db.to_string()).expect("seed legacy learn database");

    let similar = soon(&home)
        .args(["learn", "similar", "deploy"])
        .output()
        .expect("search legacy learn database");
    let stdout = String::from_utf8(similar.stdout).expect("UTF-8 similar output");

    let _ = std::fs::remove_dir_all(&home);
    assert!(
        similar.status.success(),
        "similar search failed: {}",
        String::from_utf8_lossy(&similar.stderr)
    );
    assert!(!stdout.contains(secret), "similar search leaked a secret");
    assert!(!stdout.contains("deploy --token"), "{stdout}");
}

#[test]
fn markov_fallback_does_not_render_sensitive_legacy_entries() {
    let home = isolated_home();
    let secret = "legacy-markov-secret";
    std::fs::write(home.join(".zsh_history"), "git status\n").expect("write Zsh history");
    let config_path = soon(&home)
        .args(["config", "path"])
        .output()
        .expect("get config path");
    let learn_path = PathBuf::from(
        String::from_utf8(config_path.stdout)
            .expect("UTF-8 config path")
            .trim(),
    )
    .parent()
    .expect("config parent")
    .join("learn.json");
    std::fs::create_dir_all(learn_path.parent().expect("learn parent"))
        .expect("create learn directory");
    let learn_db = serde_json::json!({
        "transitions": {
            "git": {format!("deploy --token {secret}"): 1}
        },
        "total_samples": 1
    });
    std::fs::write(&learn_path, learn_db.to_string()).expect("seed legacy learn database");

    let prediction = soon(&home)
        .args(["--shell", "zsh", "learn", "predict"])
        .output()
        .expect("predict from legacy learn database");
    let stdout = String::from_utf8(prediction.stdout).expect("UTF-8 prediction output");

    let _ = std::fs::remove_dir_all(&home);
    assert!(
        prediction.status.success(),
        "learn prediction failed: {}",
        String::from_utf8_lossy(&prediction.stderr)
    );
    assert!(!stdout.contains(secret), "Markov fallback leaked a secret");
    assert!(!stdout.contains("deploy --token"), "{stdout}");
}
