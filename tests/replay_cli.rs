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
        "soon-replay-test-{}-{nonce}-{sequence}",
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
    text: &str,
    exit_code: &str,
    previous_id: Option<&str>,
) {
    let mut args = vec![
        "events",
        "record-command",
        "--id",
        id,
        "--command",
        text,
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

fn record_suggestion(
    home: &PathBuf,
    id: &str,
    command_event_id: &str,
    source: &str,
    text: &str,
    model_outcome: Option<&str>,
) {
    let mut args = vec![
        "events",
        "record-suggestion",
        "--id",
        id,
        "--command-event-id",
        command_event_id,
        "--trigger",
        "repair",
        "--candidate-source",
        source,
        "--command",
        text,
        "--outcome",
        "shown",
        "--latency-ms",
        "7.5",
    ];
    if let Some(model_outcome) = model_outcome {
        args.extend(["--model-outcome", model_outcome]);
    }

    let output = soon(home)
        .args(args)
        .output()
        .expect("record suggestion event");
    assert!(
        output.status.success(),
        "record suggestion failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn record_suggestion_outcome(
    home: &PathBuf,
    id: &str,
    command_event_id: &str,
    text: &str,
    outcome: &str,
) {
    let output = soon(home)
        .args([
            "events",
            "record-suggestion",
            "--id",
            id,
            "--command-event-id",
            command_event_id,
            "--trigger",
            "next-step",
            "--candidate-source",
            "contextual-policy",
            "--command",
            text,
            "--outcome",
            outcome,
            "--latency-ms",
            "1.0",
        ])
        .output()
        .expect("record suggestion outcome");
    assert!(
        output.status.success(),
        "record suggestion outcome failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn chronological_replay_never_learns_from_the_sample_being_scored() {
    let home = isolated_home();
    let commands = [
        ("command-1", "git status", None),
        ("command-2", "cargo test", Some("command-1")),
        ("command-3", "cargo fmt", Some("command-2")),
        ("command-4", "git status", Some("command-3")),
        ("command-5", "cargo test", Some("command-4")),
    ];
    for (id, text, previous_id) in commands {
        record_command(&home, id, text, "0", previous_id);
    }

    let replay = soon(&home).arg("replay").output().expect("replay events");
    let stdout = String::from_utf8(replay.stdout).expect("UTF-8 replay output");

    let _ = std::fs::remove_dir_all(&home);
    assert!(
        replay.status.success(),
        "replay failed: {}",
        String::from_utf8_lossy(&replay.stderr)
    );
    assert!(stdout.contains("Samples: 4"), "{stdout}");
    assert!(stdout.contains("Coverage: 25.0% (1/4)"), "{stdout}");
    assert!(stdout.contains("Top-1 match: 25.0% (1/4)"), "{stdout}");
    assert!(stdout.contains("p50 latency:"), "{stdout}");
    assert!(stdout.contains("p95 latency:"), "{stdout}");
    assert!(stdout.contains("Zsh p95 budget: PASS"), "{stdout}");
    assert!(stdout.contains("next-step"), "{stdout}");
    assert!(stdout.contains("history"), "{stdout}");
    for (_, text, _) in commands {
        assert!(
            !stdout.contains(text),
            "replay leaked command text: {stdout}"
        );
    }
}

#[test]
fn replay_compares_the_contextual_policy_with_the_v04_baseline() {
    let home = isolated_home();
    let commands = [
        ("command-1", "git status", None),
        ("command-2", "cargo test --workspace", Some("command-1")),
        ("command-3", "git status", Some("command-2")),
        ("command-4", "cargo test --workspace", Some("command-3")),
    ];
    for (id, text, previous_id) in commands {
        record_command(&home, id, text, "0", previous_id);
    }

    let replay = soon(&home).arg("replay").output().expect("replay events");
    let stdout = String::from_utf8(replay.stdout).expect("UTF-8 replay output");

    let _ = std::fs::remove_dir_all(&home);
    assert!(replay.status.success(), "replay failed");
    let baseline = stdout
        .lines()
        .find(|line| line.trim_start().starts_with("v0.4-baseline"))
        .expect("v0.4 baseline row");
    let contextual = stdout
        .lines()
        .find(|line| line.trim_start().starts_with("contextual-policy"))
        .expect("contextual policy row");
    assert!(baseline.contains("samples=3"), "{stdout}");
    assert!(baseline.contains("coverage="), "{stdout}");
    assert!(baseline.contains("top-1="), "{stdout}");
    assert!(baseline.contains("p50="), "{stdout}");
    assert!(baseline.contains("p95="), "{stdout}");
    assert!(contextual.contains("samples=3"), "{stdout}");
    assert!(contextual.contains("coverage="), "{stdout}");
    assert!(contextual.contains("top-1="), "{stdout}");
    assert!(contextual.contains("p50="), "{stdout}");
    assert!(contextual.contains("p95="), "{stdout}");
    assert!(
        stdout.contains("Configured policy: contextual-policy"),
        "{stdout}"
    );
    assert!(stdout.contains("Contextual promotion gate:"), "{stdout}");
    assert!(
        stdout.contains("requires top-1 > baseline and p95 <= 20 ms"),
        "{stdout}"
    );
    for (_, text, _) in commands {
        assert!(
            !stdout.contains(text),
            "replay leaked command text: {stdout}"
        );
    }
}

#[test]
fn replay_breaks_metrics_down_by_manual_next_step_and_repair_trigger() {
    let home = isolated_home();
    std::fs::write(home.join("manual-history"), "git status\ncargo test\n")
        .expect("write import fixture");
    let import = soon(&home)
        .args([
            "events",
            "import-zsh",
            "--path",
            home.join("manual-history").to_str().expect("UTF-8 path"),
        ])
        .output()
        .expect("import Zsh history");
    assert!(import.status.success(), "import failed");

    record_command(&home, "failed", "cargo build", "1", None);
    record_command(
        &home,
        "repair",
        "cargo build --verbose",
        "0",
        Some("failed"),
    );
    record_command(&home, "next", "git diff --stat", "0", Some("repair"));

    let replay = soon(&home).arg("replay").output().expect("replay events");
    let stdout = String::from_utf8(replay.stdout).expect("UTF-8 replay output");

    let _ = std::fs::remove_dir_all(&home);
    assert!(replay.status.success(), "replay failed");
    assert!(stdout.contains("manual    samples=1"), "{stdout}");
    assert!(stdout.contains("next-step samples=1"), "{stdout}");
    assert!(stdout.contains("repair    samples=1"), "{stdout}");
}

#[test]
fn recorded_sources_and_model_failures_are_scored_only_after_they_occur() {
    let home = isolated_home();
    let actual = "cargo test -- --nocapture";
    record_command(&home, "failed", "cargo test", "1", None);
    record_suggestion(
        &home,
        "local-success",
        "failed",
        "local-model",
        actual,
        Some("success"),
    );
    record_suggestion(
        &home,
        "remote-timeout",
        "failed",
        "remote-provider",
        "cargo check",
        Some("timeout"),
    );
    record_suggestion(
        &home,
        "local-invalid",
        "failed",
        "local-model",
        "cargo clippy",
        Some("invalid-output"),
    );
    record_suggestion(
        &home,
        "remote-fallback",
        "failed",
        "remote-provider",
        actual,
        Some("deterministic-fallback"),
    );
    record_suggestion(
        &home,
        "contextual",
        "failed",
        "contextual-policy",
        actual,
        None,
    );
    record_command(&home, "fixed", actual, "0", Some("failed"));
    record_suggestion(
        &home,
        "too-late",
        "failed",
        "late-unknown-source",
        actual,
        None,
    );

    let replay = soon(&home).arg("replay").output().expect("replay events");
    let stdout = String::from_utf8(replay.stdout).expect("UTF-8 replay output");

    let _ = std::fs::remove_dir_all(&home);
    assert!(replay.status.success(), "replay failed");
    let local_model = stdout
        .lines()
        .find(|line| line.contains("local-model"))
        .expect("local model row");
    assert!(
        local_model.contains("samples=2 coverage=100.0% top-1=50.0%"),
        "{stdout}"
    );
    let remote_provider = stdout
        .lines()
        .find(|line| line.contains("remote-provider"))
        .expect("remote provider row");
    assert!(
        remote_provider.contains("samples=2 coverage=100.0% top-1=50.0%"),
        "{stdout}"
    );
    let contextual = stdout
        .lines()
        .find(|line| line.contains("contextual-policy"))
        .expect("contextual policy row");
    assert!(
        contextual.contains("samples=1 coverage=100.0% top-1=100.0%"),
        "{stdout}"
    );
    assert!(
        !stdout.lines().any(|line| line.starts_with("  other")),
        "{stdout}"
    );
    assert!(stdout.contains("Attempts: 4"), "{stdout}");
    assert!(stdout.contains("Timeout: 25.0% (1/4)"), "{stdout}");
    assert!(stdout.contains("Invalid output: 25.0% (1/4)"), "{stdout}");
    assert!(
        stdout.contains("Deterministic fallback: 25.0% (1/4)"),
        "{stdout}"
    );
    assert!(
        !stdout.contains(actual),
        "replay leaked command text: {stdout}"
    );
}

#[test]
fn contextual_replay_uses_feedback_available_before_the_actual_command() {
    let home = isolated_home();
    let stable = "just deploy --env stable";
    record_command(&home, "stable-previous", "cargo test", "0", None);
    record_command(&home, "stable-next", stable, "0", Some("stable-previous"));
    record_command(&home, "recent-previous", "cargo test", "0", None);
    record_command(
        &home,
        "recent-next",
        "just deploy --env recent",
        "0",
        Some("recent-previous"),
    );
    record_suggestion_outcome(
        &home,
        "accepted-stable",
        "recent-previous",
        stable,
        "accepted",
    );
    record_command(&home, "current", "cargo test", "0", None);
    record_command(&home, "actual", stable, "0", Some("current"));

    let replay = soon(&home).arg("replay").output().expect("replay events");
    let stdout = String::from_utf8(replay.stdout).expect("UTF-8 replay output");

    let _ = std::fs::remove_dir_all(&home);
    assert!(replay.status.success(), "replay failed");
    let contextual = stdout
        .lines()
        .find(|line| line.trim_start().starts_with("contextual-policy"))
        .expect("contextual policy row");
    assert!(
        contextual.contains("samples=3 coverage=66.7% top-1=33.3%"),
        "{stdout}"
    );
    assert!(!stdout.contains(stable), "replay leaked command text");
}
