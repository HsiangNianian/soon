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
    record_command_in(
        home,
        id,
        command,
        exit_code,
        previous_id,
        "/tmp/soon-project",
    );
}

fn record_command_in(
    home: &PathBuf,
    id: &str,
    command: &str,
    exit_code: &str,
    previous_id: Option<&str>,
    cwd: &str,
) {
    record_command_at(home, id, command, exit_code, previous_id, cwd, 1000, 25);
}

#[allow(clippy::too_many_arguments)]
fn record_command_at(
    home: &PathBuf,
    id: &str,
    command: &str,
    exit_code: &str,
    previous_id: Option<&str>,
    cwd: &str,
    started_at_ms: i64,
    duration_ms: u64,
) {
    let started_at_ms = started_at_ms.to_string();
    let duration_ms = duration_ms.to_string();
    let mut args = vec![
        "events",
        "record-command",
        "--id",
        id,
        "--command",
        command,
        "--cwd",
        cwd,
        "--started-at-ms",
        &started_at_ms,
        "--duration-ms",
        &duration_ms,
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

fn record_suggestion_outcome(
    home: &PathBuf,
    id: &str,
    command_event_id: &str,
    command: &str,
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
            command,
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
fn contextual_policy_can_be_enabled_to_rank_by_working_directory() {
    let home = isolated_home();
    record_command_in(&home, "a-previous", "cargo test", "0", None, "/work/a");
    record_command_in(
        &home,
        "a-next",
        "just deploy --env a",
        "0",
        Some("a-previous"),
        "/work/a",
    );
    record_command_in(&home, "b1-previous", "cargo test", "0", None, "/work/b");
    record_command_in(
        &home,
        "b1-next",
        "just deploy --env b",
        "0",
        Some("b1-previous"),
        "/work/b",
    );
    record_command_in(&home, "b2-previous", "cargo test", "0", None, "/work/b");
    record_command_in(
        &home,
        "b2-next",
        "just deploy --env b",
        "0",
        Some("b2-previous"),
        "/work/b",
    );

    let configure_baseline = soon(&home)
        .args(["config", "set", "prediction.policy", "v0.4-baseline"])
        .output()
        .expect("enable v0.4 baseline");
    assert!(configure_baseline.status.success(), "configuration failed");

    let baseline = soon(&home)
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
            "/work/a",
        ])
        .output()
        .expect("predict with baseline");
    assert!(baseline.status.success(), "baseline prediction failed");
    assert_eq!(
        String::from_utf8(baseline.stdout).expect("UTF-8 baseline"),
        "just deploy --env b\n"
    );

    let configure = soon(&home)
        .args(["config", "set", "prediction.policy", "contextual"])
        .output()
        .expect("enable contextual policy");
    assert!(
        configure.status.success(),
        "configuration failed: {}",
        String::from_utf8_lossy(&configure.stderr)
    );
    let contextual = soon(&home)
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
            "/work/a",
        ])
        .output()
        .expect("predict with contextual policy");

    let _ = std::fs::remove_dir_all(&home);
    assert!(contextual.status.success(), "contextual prediction failed");
    assert_eq!(
        String::from_utf8(contextual.stdout).expect("UTF-8 contextual"),
        "just deploy --env a\n"
    );
}

#[test]
fn contextual_policy_distinguishes_failure_repair_from_success_next_step() {
    let home = isolated_home();
    record_command(&home, "failed", "cargo build", "1", None);
    record_command(
        &home,
        "repair",
        "cargo build --verbose",
        "0",
        Some("failed"),
    );
    record_command(&home, "success", "cargo build", "0", Some("repair"));
    record_command(&home, "next-step", "git diff --stat", "0", Some("success"));
    let configure = soon(&home)
        .args(["config", "set", "prediction.policy", "contextual"])
        .output()
        .expect("enable contextual policy");
    assert!(configure.status.success(), "configuration failed");

    let repair = soon(&home)
        .args([
            "--shell",
            "zsh",
            "now",
            "--raw",
            "--after",
            "cargo build",
            "--exit-code",
            "1",
            "--cwd",
            "/tmp/soon-project",
        ])
        .output()
        .expect("predict repair");
    let next_step = soon(&home)
        .args([
            "--shell",
            "zsh",
            "now",
            "--raw",
            "--after",
            "cargo build",
            "--exit-code",
            "0",
            "--cwd",
            "/tmp/soon-project",
        ])
        .output()
        .expect("predict next step");

    let _ = std::fs::remove_dir_all(&home);
    assert!(repair.status.success(), "repair prediction failed");
    assert!(next_step.status.success(), "next-step prediction failed");
    assert_eq!(
        String::from_utf8(repair.stdout).expect("UTF-8 repair"),
        "cargo build --verbose\n"
    );
    assert_eq!(
        String::from_utf8(next_step.stdout).expect("UTF-8 next-step"),
        "git diff --stat\n"
    );
}

#[test]
fn contextual_policy_uses_the_original_event_hour_bucket() {
    let home = isolated_home();
    record_command_at(
        &home,
        "day-previous",
        "cargo test",
        "0",
        None,
        "/work/project",
        32_400_000,
        25,
    );
    record_command_at(
        &home,
        "day-next",
        "just deploy --window day",
        "0",
        Some("day-previous"),
        "/work/project",
        36_000_000,
        25,
    );
    record_command_at(
        &home,
        "night-previous",
        "cargo test",
        "0",
        Some("day-next"),
        "/work/project",
        75_600_000,
        25,
    );
    record_command_at(
        &home,
        "night-next",
        "just deploy --window night",
        "0",
        Some("night-previous"),
        "/work/project",
        79_200_000,
        25,
    );
    record_command_at(
        &home,
        "current",
        "cargo test",
        "0",
        Some("night-next"),
        "/work/project",
        640_800_000,
        25,
    );
    let configure = soon(&home)
        .args(["config", "set", "prediction.policy", "contextual"])
        .output()
        .expect("enable contextual policy");
    assert!(configure.status.success(), "configuration failed");

    let prediction = soon(&home)
        .args([
            "--shell",
            "zsh",
            "now",
            "--raw",
            "--after",
            "cargo test",
            "--event-id",
            "current",
            "--exit-code",
            "0",
            "--cwd",
            "/work/project",
        ])
        .output()
        .expect("predict by original time");

    let _ = std::fs::remove_dir_all(&home);
    assert!(
        prediction.status.success(),
        "time-aware prediction failed: {}",
        String::from_utf8_lossy(&prediction.stderr)
    );
    assert_eq!(
        String::from_utf8(prediction.stdout).expect("UTF-8 prediction"),
        "just deploy --window day\n"
    );
}

#[test]
fn contextual_policy_uses_second_order_command_transitions() {
    let home = isolated_home();
    record_command(&home, "feature-before", "git add .", "0", None);
    record_command(
        &home,
        "feature-commit",
        "git commit",
        "0",
        Some("feature-before"),
    );
    record_command(
        &home,
        "feature-push",
        "git push origin feature",
        "0",
        Some("feature-commit"),
    );
    record_command(&home, "main-before", "git pull", "0", None);
    record_command(&home, "main-commit", "git commit", "0", Some("main-before"));
    record_command(
        &home,
        "main-push",
        "git push origin main",
        "0",
        Some("main-commit"),
    );
    record_command(&home, "current-before", "git add .", "0", None);
    record_command(&home, "current", "git commit", "0", Some("current-before"));
    let configure = soon(&home)
        .args(["config", "set", "prediction.policy", "contextual"])
        .output()
        .expect("enable contextual policy");
    assert!(configure.status.success(), "configuration failed");

    let prediction = soon(&home)
        .args([
            "--shell",
            "zsh",
            "now",
            "--raw",
            "--after",
            "git commit",
            "--event-id",
            "current",
            "--exit-code",
            "0",
            "--cwd",
            "/tmp/soon-project",
        ])
        .output()
        .expect("predict from second-order transition");

    let _ = std::fs::remove_dir_all(&home);
    assert!(prediction.status.success(), "prediction failed");
    assert_eq!(
        String::from_utf8(prediction.stdout).expect("UTF-8 prediction"),
        "git push origin feature\n"
    );
}

#[test]
fn contextual_policy_uses_accepted_and_executed_feedback() {
    let home = isolated_home();
    record_command(&home, "accepted-previous", "cargo test", "0", None);
    record_command(
        &home,
        "accepted-next",
        "just deploy --env stable",
        "0",
        Some("accepted-previous"),
    );
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
        "feedback-1",
        "accepted-previous",
        "just deploy --env stable",
        "accepted",
    );
    record_suggestion_outcome(
        &home,
        "feedback-1",
        "accepted-previous",
        "just deploy --env stable",
        "executed",
    );
    let configure = soon(&home)
        .args(["config", "set", "prediction.policy", "contextual"])
        .output()
        .expect("enable contextual policy");
    assert!(configure.status.success(), "configuration failed");

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
        .expect("predict from feedback");

    let _ = std::fs::remove_dir_all(&home);
    assert!(prediction.status.success(), "prediction failed");
    assert_eq!(
        String::from_utf8(prediction.stdout).expect("UTF-8 prediction"),
        "just deploy --env stable\n"
    );
}

#[test]
fn contextual_debug_explains_signal_groups_without_claiming_confidence() {
    let home = isolated_home();
    let candidate = "cargo test --workspace";
    record_command(&home, "previous", "git status", "0", None);
    record_command(&home, "next", candidate, "0", Some("previous"));
    record_suggestion_outcome(&home, "feedback", "previous", candidate, "accepted");
    let configure = soon(&home)
        .args(["config", "set", "prediction.policy", "contextual"])
        .output()
        .expect("enable contextual policy");
    assert!(configure.status.success(), "configuration failed");

    let prediction = soon(&home)
        .args([
            "--debug",
            "--shell",
            "zsh",
            "now",
            "--raw",
            "--after",
            "git status",
            "--exit-code",
            "0",
            "--cwd",
            "/tmp/soon-project",
        ])
        .output()
        .expect("debug contextual prediction");
    let stdout = String::from_utf8(prediction.stdout).expect("UTF-8 stdout");
    let stderr = String::from_utf8(prediction.stderr).expect("UTF-8 stderr");

    let _ = std::fs::remove_dir_all(&home);
    assert!(prediction.status.success(), "prediction failed: {stderr}");
    assert_eq!(stdout, format!("{candidate}\n"));
    assert!(stderr.contains("Policy: contextual-policy"), "{stderr}");
    assert!(
        stderr.contains("Candidate source: event-history"),
        "{stderr}"
    );
    assert!(stderr.contains("Signal groups:"), "{stderr}");
    assert!(stderr.contains("transition"), "{stderr}");
    assert!(stderr.contains("feedback"), "{stderr}");
    assert!(
        !stderr.to_ascii_lowercase().contains("confidence"),
        "{stderr}"
    );
    assert!(!stderr.contains(candidate), "debug leaked candidate text");
}

#[test]
fn contextual_policy_uses_the_previous_command_duration_bucket() {
    let home = isolated_home();
    record_command_at(
        &home,
        "short-previous",
        "cargo test",
        "0",
        None,
        "/work/project",
        1_000,
        200,
    );
    record_command_at(
        &home,
        "short-next",
        "terminal-notifier -message short",
        "0",
        Some("short-previous"),
        "/work/project",
        2_000,
        25,
    );
    record_command_at(
        &home,
        "long-previous",
        "cargo test",
        "0",
        None,
        "/work/project",
        3_000,
        90_000,
    );
    record_command_at(
        &home,
        "long-next",
        "terminal-notifier -message long",
        "0",
        Some("long-previous"),
        "/work/project",
        4_000,
        25,
    );
    record_command_at(
        &home,
        "current",
        "cargo test",
        "0",
        None,
        "/work/project",
        5_000,
        300,
    );
    let configure = soon(&home)
        .args(["config", "set", "prediction.policy", "contextual"])
        .output()
        .expect("enable contextual policy");
    assert!(configure.status.success(), "configuration failed");

    let prediction = soon(&home)
        .args([
            "--shell",
            "zsh",
            "now",
            "--raw",
            "--after",
            "cargo test",
            "--event-id",
            "current",
            "--exit-code",
            "0",
            "--cwd",
            "/work/project",
        ])
        .output()
        .expect("predict by duration");

    let _ = std::fs::remove_dir_all(&home);
    assert!(prediction.status.success(), "prediction failed");
    assert_eq!(
        String::from_utf8(prediction.stdout).expect("UTF-8 prediction"),
        "terminal-notifier -message short\n"
    );
}

#[test]
fn contextual_policy_uses_the_original_event_weekday() {
    let home = isolated_home();
    record_command_at(
        &home,
        "thursday-previous",
        "git fetch",
        "0",
        None,
        "/work/project",
        32_400_000,
        25,
    );
    record_command_at(
        &home,
        "thursday-next",
        "just report --day thursday",
        "0",
        Some("thursday-previous"),
        "/work/project",
        36_000_000,
        25,
    );
    record_command_at(
        &home,
        "friday-previous",
        "git fetch",
        "0",
        None,
        "/work/project",
        118_800_000,
        25,
    );
    record_command_at(
        &home,
        "friday-next",
        "just report --day friday",
        "0",
        Some("friday-previous"),
        "/work/project",
        122_400_000,
        25,
    );
    record_command_at(
        &home,
        "current",
        "git fetch",
        "0",
        None,
        "/work/project",
        640_800_000,
        25,
    );
    let configure = soon(&home)
        .args(["config", "set", "prediction.policy", "contextual"])
        .output()
        .expect("enable contextual policy");
    assert!(configure.status.success(), "configuration failed");

    let prediction = soon(&home)
        .args([
            "--shell",
            "zsh",
            "now",
            "--raw",
            "--after",
            "git fetch",
            "--event-id",
            "current",
            "--exit-code",
            "0",
            "--cwd",
            "/work/project",
        ])
        .output()
        .expect("predict by weekday");

    let _ = std::fs::remove_dir_all(&home);
    assert!(prediction.status.success(), "prediction failed");
    assert_eq!(
        String::from_utf8(prediction.stdout).expect("UTF-8 prediction"),
        "just report --day thursday\n"
    );
}

#[test]
fn contextual_policy_uses_repository_and_branch_when_known() {
    let home = isolated_home();
    let project_a = home.join("work/project-a");
    let project_b = home.join("work/project-b");
    std::fs::create_dir_all(project_a.join(".git")).expect("create project A git dir");
    std::fs::create_dir_all(project_b.join(".git")).expect("create project B git dir");
    std::fs::write(project_a.join(".git/HEAD"), "ref: refs/heads/feature\n")
        .expect("write project A feature branch");
    std::fs::write(project_b.join(".git/HEAD"), "ref: refs/heads/feature\n")
        .expect("write project B feature branch");
    let project_a = project_a.to_str().expect("UTF-8 project A");
    let project_b = project_b.to_str().expect("UTF-8 project B");

    record_command_in(&home, "a-previous", "cargo test", "0", None, project_a);
    record_command_in(
        &home,
        "a-next",
        "just deploy project-a-feature",
        "0",
        Some("a-previous"),
        project_a,
    );
    record_command_in(&home, "b-previous", "cargo test", "0", None, project_b);
    record_command_in(
        &home,
        "b-next",
        "just deploy project-b-feature",
        "0",
        Some("b-previous"),
        project_b,
    );
    std::fs::write(
        std::path::Path::new(project_a).join(".git/HEAD"),
        "ref: refs/heads/main\n",
    )
    .expect("switch project A to main");
    record_command_in(&home, "main-previous", "cargo test", "0", None, project_a);
    record_command_in(
        &home,
        "main-next",
        "just deploy project-a-main",
        "0",
        Some("main-previous"),
        project_a,
    );
    std::fs::write(
        std::path::Path::new(project_a).join(".git/HEAD"),
        "ref: refs/heads/feature\n",
    )
    .expect("switch project A back to feature");
    record_command_in(&home, "current", "cargo test", "0", None, project_a);
    let configure = soon(&home)
        .args(["config", "set", "prediction.policy", "contextual"])
        .output()
        .expect("enable contextual policy");
    assert!(configure.status.success(), "configuration failed");

    let prediction = soon(&home)
        .args([
            "--shell",
            "zsh",
            "now",
            "--raw",
            "--after",
            "cargo test",
            "--event-id",
            "current",
            "--exit-code",
            "0",
            "--cwd",
            project_a,
        ])
        .output()
        .expect("predict by repository and branch");

    let _ = std::fs::remove_dir_all(&home);
    assert!(prediction.status.success(), "prediction failed");
    assert_eq!(
        String::from_utf8(prediction.stdout).expect("UTF-8 prediction"),
        "just deploy project-a-feature\n"
    );
}

#[test]
fn contextual_policy_combines_smoothed_evidence_instead_of_one_lexicographic_signal() {
    let home = isolated_home();
    record_command_at(
        &home,
        "consistent-previous",
        "cargo test",
        "0",
        None,
        "/work/other",
        32_400_000,
        200,
    );
    record_command_at(
        &home,
        "consistent-next",
        "just deploy --evidence consistent",
        "0",
        Some("consistent-previous"),
        "/work/other",
        36_000_000,
        25,
    );
    record_suggestion_outcome(
        &home,
        "accepted-consistent",
        "consistent-previous",
        "just deploy --evidence consistent",
        "accepted",
    );
    record_command_at(
        &home,
        "directory-previous",
        "cargo test",
        "1",
        None,
        "/work/current",
        118_800_000,
        90_000,
    );
    record_command_at(
        &home,
        "directory-next",
        "just deploy --evidence directory-only",
        "0",
        Some("directory-previous"),
        "/work/current",
        165_600_000,
        25,
    );
    record_command_at(
        &home,
        "current",
        "cargo test",
        "0",
        None,
        "/work/current",
        640_800_000,
        200,
    );
    let configure = soon(&home)
        .args(["config", "set", "prediction.policy", "contextual"])
        .output()
        .expect("enable contextual policy");
    assert!(configure.status.success(), "configuration failed");

    let prediction = soon(&home)
        .args([
            "--shell",
            "zsh",
            "now",
            "--raw",
            "--after",
            "cargo test",
            "--event-id",
            "current",
            "--exit-code",
            "0",
            "--cwd",
            "/work/current",
        ])
        .output()
        .expect("predict from combined evidence");

    let _ = std::fs::remove_dir_all(&home);
    assert!(prediction.status.success(), "prediction failed");
    assert_eq!(
        String::from_utf8(prediction.stdout).expect("UTF-8 prediction"),
        "just deploy --evidence consistent\n"
    );
}

#[test]
fn contextual_policy_treats_missing_historical_metadata_as_no_evidence() {
    let home = isolated_home();
    let history = home.join("plain-history");
    std::fs::write(&history, "git status\ncargo test --workspace\n").expect("write plain history");
    let import = soon(&home)
        .args([
            "events",
            "import-zsh",
            "--path",
            history.to_str().expect("UTF-8 history path"),
        ])
        .output()
        .expect("import history without metadata");
    assert!(import.status.success(), "history import failed");
    let configure = soon(&home)
        .args(["config", "set", "prediction.policy", "contextual"])
        .output()
        .expect("enable contextual policy");
    assert!(configure.status.success(), "configuration failed");

    let prediction = soon(&home)
        .args([
            "--debug",
            "--shell",
            "zsh",
            "now",
            "--raw",
            "--after",
            "git status",
            "--exit-code",
            "0",
            "--cwd",
            "/work/unknown",
        ])
        .output()
        .expect("predict with missing metadata");
    let stdout = String::from_utf8(prediction.stdout).expect("UTF-8 stdout");
    let stderr = String::from_utf8(prediction.stderr).expect("UTF-8 stderr");

    let _ = std::fs::remove_dir_all(&home);
    assert!(prediction.status.success(), "prediction failed: {stderr}");
    assert_eq!(stdout, "cargo test --workspace\n");
    assert!(
        stderr.contains("Signal groups: transition, frequency, recency"),
        "{stderr}"
    );
    for absent in ["directory", "time", "result", "duration"] {
        assert!(
            !stderr.contains(absent),
            "invented {absent} evidence: {stderr}"
        );
    }
}

#[test]
fn contextual_policy_ordering_is_deterministic_across_processes() {
    let home = isolated_home();
    record_command(&home, "first-previous", "git status", "0", None);
    record_command(
        &home,
        "first-next",
        "cargo test --package first",
        "0",
        Some("first-previous"),
    );
    record_command(&home, "second-previous", "git status", "0", None);
    record_command(
        &home,
        "second-next",
        "cargo test --package second",
        "0",
        Some("second-previous"),
    );
    let configure = soon(&home)
        .args(["config", "set", "prediction.policy", "contextual"])
        .output()
        .expect("enable contextual policy");
    assert!(configure.status.success(), "configuration failed");

    let mut predictions = Vec::new();
    for _ in 0..8 {
        let prediction = soon(&home)
            .args([
                "--shell",
                "zsh",
                "now",
                "--raw",
                "--after",
                "git status",
                "--exit-code",
                "0",
                "--cwd",
                "/tmp/soon-project",
            ])
            .output()
            .expect("repeat contextual prediction");
        assert!(prediction.status.success(), "prediction failed");
        predictions.push(String::from_utf8(prediction.stdout).expect("UTF-8 prediction"));
    }

    let _ = std::fs::remove_dir_all(&home);
    assert!(
        predictions
            .iter()
            .all(|prediction| prediction == &predictions[0]),
        "predictions changed across processes: {predictions:?}"
    );
    assert_eq!(predictions[0], "cargo test --package second\n");
}

#[test]
fn raw_prediction_can_report_the_selected_policy_source() {
    let home = isolated_home();
    record_command(&home, "previous", "git status", "0", None);
    record_command(
        &home,
        "next",
        "cargo test --workspace",
        "0",
        Some("previous"),
    );
    let configure = soon(&home)
        .args(["config", "set", "prediction.policy", "contextual"])
        .output()
        .expect("enable contextual policy");
    assert!(configure.status.success(), "configuration failed");

    let prediction = soon(&home)
        .args([
            "--shell",
            "zsh",
            "now",
            "--raw",
            "--include-source",
            "--after",
            "git status",
            "--exit-code",
            "0",
            "--cwd",
            "/tmp/soon-project",
        ])
        .output()
        .expect("predict with source metadata");

    let _ = std::fs::remove_dir_all(&home);
    assert!(prediction.status.success(), "prediction failed");
    assert_eq!(
        String::from_utf8(prediction.stdout).expect("UTF-8 prediction"),
        "contextual-policy\tcargo test --workspace\n"
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

#[test]
fn zsh_history_import_preview_is_non_mutating_and_private() {
    let home = isolated_home();
    let history_path = home.join("history.zsh");
    let sensitive = "preview-secret-value";
    let malformed = "malformed-command-must-not-print";
    std::fs::write(
        &history_path,
        format!(
            "git status\n: 1721811600:12;cargo test --workspace\n: bad:line;{malformed}\nexport API_TOKEN={sensitive}\n"
        ),
    )
    .expect("write Zsh history fixture");

    let preview = soon(&home)
        .args(["events", "import-zsh", "--path"])
        .arg(&history_path)
        .arg("--preview")
        .output()
        .expect("preview Zsh history import");
    let stdout = String::from_utf8(preview.stdout).expect("UTF-8 import preview");

    assert!(
        preview.status.success(),
        "preview failed: {}",
        String::from_utf8_lossy(&preview.stderr)
    );
    assert!(stdout.contains("Importable entries: 2"), "{stdout}");
    assert!(stdout.contains("Sensitive entries skipped: 1"), "{stdout}");
    assert!(stdout.contains("Malformed entries skipped: 1"), "{stdout}");
    assert!(stdout.contains("Would import: 2"), "{stdout}");
    assert!(!stdout.contains(sensitive), "preview leaked sensitive text");
    assert!(!stdout.contains(malformed), "preview leaked malformed text");

    let inspect = soon(&home)
        .args(["events", "inspect"])
        .output()
        .expect("inspect after preview");
    let inspect_stdout = String::from_utf8(inspect.stdout).expect("UTF-8 inspect output");

    let _ = std::fs::remove_dir_all(&home);
    assert!(
        inspect_stdout.contains("Command events: 0"),
        "{inspect_stdout}"
    );
}

#[test]
fn zsh_history_import_is_idempotent_and_preserves_unknown_metadata() {
    let home = isolated_home();
    let history_path = home.join("history.zsh");
    std::fs::write(
        &history_path,
        "git status\n: 1721811600:12;cargo test --workspace\n",
    )
    .expect("write Zsh history fixture");

    let first = soon(&home)
        .args(["events", "import-zsh", "--path"])
        .arg(&history_path)
        .output()
        .expect("import Zsh history");
    assert!(
        first.status.success(),
        "first import failed: {}",
        String::from_utf8_lossy(&first.stderr)
    );
    let first_stdout = String::from_utf8(first.stdout).expect("UTF-8 import output");
    assert!(first_stdout.contains("Imported: 2"), "{first_stdout}");
    assert!(
        first_stdout.contains("Already imported: 0"),
        "{first_stdout}"
    );

    let second = soon(&home)
        .args(["events", "import-zsh", "--path"])
        .arg(&history_path)
        .output()
        .expect("repeat Zsh history import");
    assert!(
        second.status.success(),
        "second import failed: {}",
        String::from_utf8_lossy(&second.stderr)
    );
    let second_stdout = String::from_utf8(second.stdout).expect("UTF-8 repeated import output");
    assert!(second_stdout.contains("Imported: 0"), "{second_stdout}");
    assert!(
        second_stdout.contains("Already imported: 2"),
        "{second_stdout}"
    );

    let inspect = soon(&home)
        .args(["events", "inspect"])
        .output()
        .expect("inspect imported events");
    let inspect_stdout = String::from_utf8(inspect.stdout).expect("UTF-8 inspect output");
    let event_path = inspect_stdout
        .lines()
        .find_map(|line| line.strip_prefix("Event store: "))
        .map(PathBuf::from)
        .expect("event store path");
    let stored = std::fs::read_to_string(event_path).expect("read imported event store");
    let events: Vec<serde_json::Value> = stored
        .lines()
        .map(|line| serde_json::from_str(line).expect("parse imported event"))
        .collect();

    let _ = std::fs::remove_dir_all(&home);
    assert!(
        inspect_stdout.contains("Command events: 2"),
        "{inspect_stdout}"
    );
    assert_eq!(events.len(), 2);
    assert_eq!(events[0]["command"], "git status");
    assert!(events[0]["cwd"].is_null());
    assert!(events[0]["started_at_ms"].is_null());
    assert!(events[0]["duration_ms"].is_null());
    assert!(events[0]["exit_code"].is_null());
    assert!(events[0]["previous_event_id"].is_null());
    assert_eq!(events[1]["command"], "cargo test --workspace");
    assert_eq!(events[1]["started_at_ms"], 1_721_811_600_000_i64);
    assert_eq!(events[1]["duration_ms"], 12_000_u64);
    assert!(events[1]["cwd"].is_null());
    assert!(events[1]["exit_code"].is_null());
    assert_eq!(events[1]["previous_event_id"], events[0]["id"]);
}

#[test]
fn imported_zsh_history_can_predict_without_the_source_history_file() {
    let home = isolated_home();
    let history_path = home.join("cold-start-history.zsh");
    std::fs::write(&history_path, "git status\ncargo test --workspace\n")
        .expect("write cold-start history");

    let imported = soon(&home)
        .args(["events", "import-zsh", "--path"])
        .arg(&history_path)
        .output()
        .expect("import cold-start history");
    assert!(
        imported.status.success(),
        "import failed: {}",
        String::from_utf8_lossy(&imported.stderr)
    );
    std::fs::remove_file(&history_path).expect("remove source history");

    let prediction = soon(&home)
        .args([
            "--shell",
            "zsh",
            "now",
            "--raw",
            "--after",
            "git status",
            "--exit-code",
            "0",
        ])
        .output()
        .expect("predict from imported events");
    let stdout = String::from_utf8(prediction.stdout).expect("UTF-8 prediction");

    let _ = std::fs::remove_dir_all(&home);
    assert!(
        prediction.status.success(),
        "prediction failed: {}",
        String::from_utf8_lossy(&prediction.stderr)
    );
    assert_eq!(stdout, "cargo test --workspace\n");
}

#[test]
fn overlapping_rotated_zsh_histories_are_deduplicated() {
    let home = isolated_home();
    let current = home.join(".zsh_history");
    let rotated = home.join(".zsh_history.1");
    let contents = "git status\ncargo test --workspace\n";
    std::fs::write(&current, contents).expect("write current Zsh history");
    std::fs::write(&rotated, contents).expect("write rotated Zsh history");

    let preview = soon(&home)
        .args(["events", "import-zsh", "--path"])
        .arg(&rotated)
        .args(["--path"])
        .arg(&current)
        .arg("--preview")
        .output()
        .expect("preview overlapping Zsh histories");
    let preview_stdout = String::from_utf8(preview.stdout).expect("UTF-8 preview");
    assert!(preview.status.success(), "preview failed");
    assert!(
        preview_stdout.contains("Importable entries: 4"),
        "{preview_stdout}"
    );
    assert!(
        preview_stdout.contains("Duplicate entries skipped: 2"),
        "{preview_stdout}"
    );
    assert!(
        preview_stdout.contains("Would import: 2"),
        "{preview_stdout}"
    );

    let imported = soon(&home)
        .args(["events", "import-zsh", "--path"])
        .arg(&rotated)
        .args(["--path"])
        .arg(&current)
        .output()
        .expect("import overlapping Zsh histories");
    let import_stdout = String::from_utf8(imported.stdout).expect("UTF-8 import output");
    assert!(imported.status.success(), "import failed");
    assert!(import_stdout.contains("Imported: 2"), "{import_stdout}");

    let inspect = soon(&home)
        .args(["events", "inspect"])
        .output()
        .expect("inspect imported histories");
    let inspect_stdout = String::from_utf8(inspect.stdout).expect("UTF-8 inspect output");

    let _ = std::fs::remove_dir_all(&home);
    assert!(
        inspect_stdout.contains("Command events: 2"),
        "{inspect_stdout}"
    );
}
