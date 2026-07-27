use std::path::PathBuf;
use std::process::{Command, Output};
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
        "soon-report-test-{}-{nonce}-{sequence}",
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

fn run(home: &PathBuf, args: &[&str]) -> Output {
    let output = soon(home).args(args).output().expect("run soon");
    assert!(
        output.status.success(),
        "soon {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn record_command(home: &PathBuf, id: &str, text: &str, previous_id: Option<&str>) {
    let mut args = vec![
        "events",
        "record-command",
        "--id",
        id,
        "--command",
        text,
        "--cwd",
        "/private/work/soon",
        "--started-at-ms",
        "1000",
        "--duration-ms",
        "25",
        "--exit-code",
        "0",
        "--shell",
        "zsh",
    ];
    if let Some(previous_id) = previous_id {
        args.extend(["--previous-id", previous_id]);
    }
    run(home, &args);
}

fn record_suggestion(home: &PathBuf, outcome: &str) {
    run(
        home,
        &[
            "events",
            "record-suggestion",
            "--id",
            "suggestion-1",
            "--command-event-id",
            "command-4",
            "--trigger",
            "next-step",
            "--candidate-source",
            "deterministic-history",
            "--command",
            "cargo test --workspace",
            "--outcome",
            outcome,
            "--latency-ms",
            "7.5",
        ],
    );
}

fn populated_home() -> PathBuf {
    let home = isolated_home();
    for (id, text, previous_id) in [
        ("command-1", "git status", None),
        ("command-2", "cargo test", Some("command-1")),
        ("command-3", "cargo fmt", Some("command-2")),
        ("command-4", "git status", Some("command-3")),
        ("command-5", "cargo test", Some("command-4")),
    ] {
        record_command(&home, id, text, previous_id);
    }
    for outcome in ["shown", "accepted", "executed"] {
        record_suggestion(&home, outcome);
    }
    home
}

#[test]
fn human_report_combines_replay_quality_with_adoption_outcomes() {
    let home = populated_home();

    let report = run(&home, &["report"]);
    let stdout = String::from_utf8(report.stdout).expect("UTF-8 report output");

    let _ = std::fs::remove_dir_all(&home);
    assert!(stdout.contains("Privacy-safe adoption report"), "{stdout}");
    assert!(stdout.contains("Samples: 4"), "{stdout}");
    assert!(
        stdout.contains("Prediction coverage: 25.0% (1/4)"),
        "{stdout}"
    );
    assert!(stdout.contains("Suggestions shown: 1"), "{stdout}");
    assert!(stdout.contains("Accepted: 1 (100.0% of shown)"), "{stdout}");
    assert!(stdout.contains("Executed: 1 (100.0% of shown)"), "{stdout}");
    assert!(stdout.contains("p50 latency:"), "{stdout}");
    assert!(stdout.contains("p95 latency:"), "{stdout}");
}

#[test]
fn json_report_has_a_versioned_aggregate_only_schema() {
    let home = populated_home();

    let report = run(&home, &["report", "--json"]);
    let json: serde_json::Value =
        serde_json::from_slice(&report.stdout).expect("report is valid JSON");

    let _ = std::fs::remove_dir_all(&home);
    assert_eq!(json.as_object().expect("JSON object").len(), 4, "{json}");
    assert_eq!(json["schema_version"], 1);
    assert_eq!(
        json["samples"],
        serde_json::json!({
            "eligible_transitions": 4,
            "predictions": 1,
            "prediction_coverage_percent": 25.0
        })
    );
    assert_eq!(
        json["suggestions"],
        serde_json::json!({
            "shown": 1,
            "accepted": 1,
            "acceptance_percent": 100.0,
            "executed": 1,
            "execution_percent": 100.0
        })
    );
    assert_eq!(
        json["latency_ms"]
            .as_object()
            .expect("latency object")
            .len(),
        2,
        "{json}"
    );
    assert!(json["latency_ms"]["p50"].is_number(), "{json}");
    assert!(json["latency_ms"]["p95"].is_number(), "{json}");
}

#[test]
fn empty_store_reports_unavailable_rates_and_latency_without_failing() {
    let home = isolated_home();

    let human = run(&home, &["report"]);
    let human = String::from_utf8(human.stdout).expect("UTF-8 report output");
    let json = run(&home, &["report", "--json"]);
    let json: serde_json::Value =
        serde_json::from_slice(&json.stdout).expect("empty report is valid JSON");

    let _ = std::fs::remove_dir_all(&home);
    assert!(human.contains("Samples: 0"), "{human}");
    assert!(human.contains("Prediction coverage: n/a (0/0)"), "{human}");
    assert!(
        human.contains("Accepted: 0 (n/a; no shown suggestions)"),
        "{human}"
    );
    assert!(
        human.contains("Executed: 0 (n/a; no shown suggestions)"),
        "{human}"
    );
    assert!(human.contains("p50 latency: n/a"), "{human}");
    assert!(human.contains("p95 latency: n/a"), "{human}");
    assert_eq!(json["samples"]["eligible_transitions"], 0);
    assert_eq!(
        json["samples"]["prediction_coverage_percent"],
        serde_json::Value::Null
    );
    assert_eq!(
        json["suggestions"]["acceptance_percent"],
        serde_json::Value::Null
    );
    assert_eq!(
        json["suggestions"]["execution_percent"],
        serde_json::Value::Null
    );
    assert_eq!(json["latency_ms"]["p50"], serde_json::Value::Null);
    assert_eq!(json["latency_ms"]["p95"], serde_json::Value::Null);
}

#[test]
fn partially_populated_store_keeps_counts_but_not_undefined_rates() {
    let home = isolated_home();
    record_command(&home, "only-command", "git status", None);
    record_suggestion(&home, "accepted");

    let report = run(&home, &["report", "--json"]);
    let json: serde_json::Value =
        serde_json::from_slice(&report.stdout).expect("partial report is valid JSON");

    let _ = std::fs::remove_dir_all(&home);
    assert_eq!(json["samples"]["eligible_transitions"], 0);
    assert_eq!(json["suggestions"]["shown"], 0);
    assert_eq!(json["suggestions"]["accepted"], 1);
    assert_eq!(
        json["suggestions"]["acceptance_percent"],
        serde_json::Value::Null
    );
    assert_eq!(json["latency_ms"]["p95"], serde_json::Value::Null);
}

#[test]
fn reports_never_emit_sensitive_values_from_legacy_event_rows() {
    let home = isolated_home();
    let store = home.join(".local/share/soon/events.jsonl");
    std::fs::create_dir_all(store.parent().expect("event directory"))
        .expect("create event directory");
    let sensitive_command = "export OPENAI_API_KEY=sk-private-report-marker";
    let sensitive_path = "/Users/alice/private-client/report-marker";
    let event_id = "private-event-id-report-marker";
    let rows = [
        serde_json::json!({
            "kind": "command",
            "schema_version": 1,
            "id": event_id,
            "command": sensitive_command,
            "cwd": sensitive_path,
            "started_at_ms": 1000,
            "duration_ms": 25,
            "exit_code": 0,
            "shell": "zsh",
            "previous_event_id": null
        }),
        serde_json::json!({
            "kind": "suggestion",
            "schema_version": 1,
            "id": "private-suggestion-id-report-marker",
            "command_event_id": event_id,
            "trigger": "next-step",
            "candidate_source": "deterministic-history",
            "command": sensitive_command,
            "outcome": "shown",
            "latency_ms": 7.5
        }),
    ];
    let encoded = rows
        .iter()
        .map(serde_json::Value::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(&store, format!("{encoded}\n")).expect("write legacy event rows");

    let human = run(&home, &["report"]);
    let json = run(&home, &["report", "--json"]);
    let combined = format!(
        "{}\n{}",
        String::from_utf8(human.stdout).expect("UTF-8 human report"),
        String::from_utf8(json.stdout).expect("UTF-8 JSON report")
    );

    let _ = std::fs::remove_dir_all(&home);
    for sensitive in [sensitive_command, sensitive_path, "alice", event_id] {
        assert!(
            !combined.contains(sensitive),
            "report leaked {sensitive}: {combined}"
        );
    }
}
