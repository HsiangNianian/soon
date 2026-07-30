use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

static NEXT_HOME: AtomicU64 = AtomicU64::new(0);

fn isolated_home() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock")
        .as_nanos();
    let sequence = NEXT_HOME.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "soon-model-test-{}-{nonce}-{sequence}",
        std::process::id()
    ));
    fs::create_dir_all(&path).expect("create isolated home");
    path
}

fn soon(home: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_soon"));
    command
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("XDG_DATA_HOME", home.join(".local/share"));
    command
}

fn set_config(home: &Path, key: &str, value: &str) {
    let output = soon(home)
        .args(["config", "set", key, value])
        .output()
        .expect("set config");
    assert!(
        output.status.success(),
        "failed to set {key}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn configure_provider(home: &Path, listener: &TcpListener, provider: &str) {
    set_config(home, "llm.provider", provider);
    set_config(
        home,
        "llm.api_url",
        &format!(
            "http://{}",
            listener.local_addr().expect("provider address")
        ),
    );
    set_config(home, "llm.model", "mock-command-model");
}

fn record_command(home: &Path, id: &str, text: &str, exit_code: &str, previous_id: Option<&str>) {
    let mut args = vec![
        "events",
        "record-command",
        "--id",
        id,
        "--command",
        text,
        "--cwd",
        "/tmp/soon-model-project",
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

fn seed_repair_history(home: &Path) {
    record_command(home, "failed-old", "cargo test", "1", None);
    record_command(
        home,
        "repair-old",
        "cargo test -- --nocapture",
        "0",
        Some("failed-old"),
    );
    record_command(
        home,
        "failed-current",
        "cargo test",
        "1",
        Some("repair-old"),
    );
}

fn spawn_provider(
    listener: TcpListener,
    commands: Vec<String>,
    delay: Duration,
) -> (thread::JoinHandle<()>, mpsc::Receiver<String>) {
    let (request_tx, request_rx) = mpsc::channel();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept provider request");
        let request = read_http_request(&mut stream);
        let _ = request_tx.send(request);
        if !delay.is_zero() {
            thread::sleep(delay);
        }
        let model_content = serde_json::json!({"commands": commands}).to_string();
        let body = serde_json::json!({
            "choices": [{"message": {"content": model_content}}]
        })
        .to_string();
        let _ = write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
    });
    (server, request_rx)
}

fn read_http_request(stream: &mut TcpStream) -> String {
    let mut request = Vec::new();
    let mut chunk = [0_u8; 4096];
    loop {
        let read = stream.read(&mut chunk).expect("read provider request");
        if read == 0 {
            break;
        }
        request.extend_from_slice(&chunk[..read]);
        let Some(header_end) = request.windows(4).position(|window| window == b"\r\n\r\n") else {
            continue;
        };
        let headers = String::from_utf8_lossy(&request[..header_end]);
        let content_length = headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().ok())
                    .flatten()
            })
            .unwrap_or(0);
        if request.len() >= header_end + 4 + content_length {
            break;
        }
    }
    String::from_utf8(request).expect("UTF-8 provider request")
}

fn append_legacy_sensitive_event(home: &Path, marker: &str) {
    let path = home.join(".local/share/soon/events.jsonl");
    fs::create_dir_all(path.parent().expect("event parent")).expect("create event parent");
    let event = serde_json::json!({
        "kind": "command",
        "schema_version": 1,
        "id": "legacy-sensitive",
        "command": format!("export API_TOKEN={marker}"),
        "cwd": "/tmp/soon-model-project",
        "started_at_ms": 1,
        "duration_ms": 1,
        "exit_code": 0,
        "shell": "zsh",
        "previous_event_id": null
    });
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .expect("open event store");
    writeln!(file, "{event}").expect("append legacy event");
}

#[test]
fn model_sources_are_off_by_default_and_do_not_contact_the_provider() {
    let home = isolated_home();
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind provider");
    listener
        .set_nonblocking(true)
        .expect("nonblocking provider");
    configure_provider(&home, &listener, "local");
    seed_repair_history(&home);

    let output = soon(&home)
        .args([
            "now",
            "--raw",
            "--include-source",
            "--after",
            "cargo test",
            "--exit-code",
            "1",
            "--event-id",
            "failed-current",
            "--cwd",
            "/tmp/soon-model-project",
        ])
        .output()
        .expect("predict without model");

    assert!(output.status.success(), "prediction failed");
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("cargo test -- --nocapture"),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        matches!(listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock),
        "default prediction contacted the provider"
    );
    let _ = fs::remove_dir_all(home);
}

#[test]
fn explicit_generation_uses_filtered_context_and_never_executes_the_candidate() {
    let home = isolated_home();
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind provider");
    configure_provider(&home, &listener, "openai-compatible");
    let secret_marker = "history-secret-must-not-leave-device";
    append_legacy_sensitive_event(&home, secret_marker);
    record_command(&home, "safe", "cargo check", "0", None);
    let marker = home.join("model-output-must-not-execute");
    let generated = format!("touch {}", marker.display());
    let (server, request_rx) = spawn_provider(listener, vec![generated.clone()], Duration::ZERO);

    let output = soon(&home)
        .args(["generate", "--raw", "--include-source"])
        .output()
        .expect("generate command");
    let request = request_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("provider request");
    server.join().expect("provider thread");

    assert!(
        output.status.success(),
        "generation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).expect("UTF-8 output"),
        format!("remote-provider\tsuccess\t{generated}\n")
    );
    assert!(request.contains("cargo check"), "{request}");
    assert!(!request.contains(secret_marker), "{request}");
    assert!(!request.contains("API_TOKEN"), "{request}");
    assert!(!marker.exists(), "model candidate was executed");
    let _ = fs::remove_dir_all(home);
}

#[test]
fn opt_in_local_repair_flows_through_the_contextual_ranker() {
    let home = isolated_home();
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind provider");
    configure_provider(&home, &listener, "local");
    set_config(&home, "prediction.model_mode", "repair");
    seed_repair_history(&home);
    let model_repair = "cargo test --workspace".to_string();
    let (server, _) = spawn_provider(listener, vec![model_repair.clone()], Duration::ZERO);

    let output = soon(&home)
        .args([
            "now",
            "--raw",
            "--include-source",
            "--after",
            "cargo test",
            "--exit-code",
            "1",
            "--event-id",
            "failed-current",
            "--cwd",
            "/tmp/soon-model-project",
        ])
        .output()
        .expect("model repair");
    server.join().expect("provider thread");

    assert!(
        output.status.success(),
        "repair failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).expect("UTF-8 output"),
        format!("local-model\tsuccess\t{model_repair}\n")
    );
    let _ = fs::remove_dir_all(home);
}

#[test]
fn opt_in_rerank_can_reorder_only_local_history_candidates() {
    let home = isolated_home();
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind provider");
    configure_provider(&home, &listener, "local");
    set_config(&home, "prediction.model_mode", "rerank");
    record_command(&home, "build-one", "cargo build", "0", None);
    record_command(&home, "test", "cargo test", "0", Some("build-one"));
    record_command(&home, "build-two", "cargo build", "0", Some("test"));
    record_command(&home, "clippy", "cargo clippy", "0", Some("build-two"));
    record_command(&home, "build-current", "cargo build", "0", Some("clippy"));
    let (server, _) = spawn_provider(
        listener,
        vec!["cargo clippy".to_string(), "cargo test".to_string()],
        Duration::ZERO,
    );

    let output = soon(&home)
        .args([
            "now",
            "--raw",
            "--include-source",
            "--after",
            "cargo build",
            "--exit-code",
            "0",
            "--event-id",
            "build-current",
            "--cwd",
            "/tmp/soon-model-project",
        ])
        .output()
        .expect("model rerank");
    server.join().expect("provider thread");

    assert!(
        output.status.success(),
        "rerank failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).expect("UTF-8 output"),
        "local-model\tsuccess\tcargo clippy\n"
    );
    let _ = fs::remove_dir_all(home);
}

#[test]
fn provider_timeout_falls_back_within_the_configured_deadline() {
    let home = isolated_home();
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind provider");
    configure_provider(&home, &listener, "local");
    set_config(&home, "prediction.model_mode", "repair");
    set_config(&home, "prediction.model_timeout_ms", "40");
    seed_repair_history(&home);
    let (server, _) = spawn_provider(
        listener,
        vec!["cargo test --workspace".to_string()],
        Duration::from_millis(200),
    );

    let started = Instant::now();
    let output = soon(&home)
        .args([
            "now",
            "--raw",
            "--include-source",
            "--after",
            "cargo test",
            "--exit-code",
            "1",
            "--event-id",
            "failed-current",
            "--cwd",
            "/tmp/soon-model-project",
        ])
        .output()
        .expect("timed model repair");
    let elapsed = started.elapsed();
    server.join().expect("provider thread");

    assert!(output.status.success(), "fallback failed");
    assert_eq!(
        String::from_utf8(output.stdout).expect("UTF-8 output"),
        "deterministic-fallback\ttimeout\tcargo test -- --nocapture\n"
    );
    assert!(elapsed < Duration::from_millis(500), "elapsed={elapsed:?}");
    let _ = fs::remove_dir_all(home);
}

#[test]
fn dangerous_model_output_is_rejected_before_rendering() {
    let home = isolated_home();
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind provider");
    configure_provider(&home, &listener, "local");
    set_config(&home, "prediction.model_mode", "repair");
    seed_repair_history(&home);
    let (server, _) = spawn_provider(listener, vec!["rm -rf /".to_string()], Duration::ZERO);

    let output = soon(&home)
        .args([
            "now",
            "--raw",
            "--include-source",
            "--after",
            "cargo test",
            "--exit-code",
            "1",
            "--event-id",
            "failed-current",
            "--cwd",
            "/tmp/soon-model-project",
        ])
        .output()
        .expect("unsafe model repair");
    server.join().expect("provider thread");

    let stdout = String::from_utf8(output.stdout).expect("UTF-8 output");
    assert!(output.status.success(), "fallback failed");
    assert_eq!(
        stdout,
        "deterministic-fallback\tinvalid-output\tcargo test -- --nocapture\n"
    );
    assert!(!stdout.contains("rm -rf"), "{stdout}");
    let _ = fs::remove_dir_all(home);
}
