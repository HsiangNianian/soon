use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
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
        "soon-zsh-test-{}-{nonce}-{sequence}",
        std::process::id()
    ));
    std::fs::create_dir_all(&path).expect("create isolated home");
    path
}

#[test]
fn init_zsh_prints_a_sourceable_script_without_noise() {
    let output = Command::new(env!("CARGO_BIN_EXE_soon"))
        .args(["init", "zsh"])
        .output()
        .expect("run soon init zsh");

    assert!(
        output.status.success(),
        "soon init zsh failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.is_empty(),
        "unexpected stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!output.stdout.is_empty(), "integration script was empty");

    let Ok(mut zsh) = Command::new("zsh")
        .arg("-n")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    else {
        return;
    };
    zsh.stdin
        .take()
        .expect("zsh stdin")
        .write_all(&output.stdout)
        .expect("write integration script to zsh");
    let syntax = zsh.wait_with_output().expect("check Zsh syntax");

    assert!(
        syntax.status.success(),
        "invalid Zsh integration: {}",
        String::from_utf8_lossy(&syntax.stderr)
    );
}

#[test]
fn raw_prediction_uses_the_submitted_command_as_context() {
    let home = isolated_home();
    std::fs::write(
        home.join(".zsh_history"),
        "git status\ncargo test --workspace\necho break\ngit diff\ncargo test --workspace\n",
    )
    .expect("write Zsh history");

    let output = Command::new(env!("CARGO_BIN_EXE_soon"))
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .args([
            "--shell", "zsh", "--ngram", "1", "now", "--raw", "--after", "git log",
        ])
        .output()
        .expect("run raw prediction");

    let _ = std::fs::remove_dir_all(&home);
    assert!(
        output.status.success(),
        "raw prediction failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty(), "raw prediction wrote diagnostics");
    assert_eq!(
        String::from_utf8(output.stdout).expect("UTF-8 prediction"),
        "cargo test --workspace\n"
    );
}

#[test]
fn manual_prediction_preserves_plain_commands_containing_semicolons() {
    let home = isolated_home();
    std::fs::write(
        home.join(".zsh_history"),
        concat!(
            "git status\n",
            "printf before; echo after\n",
            "echo first-break\n",
            "git diff\n",
            "printf before; echo after\n",
            "echo second-break\n",
            "git log\n",
            "cargo test --workspace\n",
            "git show\n",
        ),
    )
    .expect("write Zsh history");

    let output = Command::new(env!("CARGO_BIN_EXE_soon"))
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .args(["--shell", "zsh", "--ngram", "1", "now"])
        .output()
        .expect("run manual prediction");
    let stdout = String::from_utf8(output.stdout).expect("UTF-8 prediction");

    let _ = std::fs::remove_dir_all(&home);
    assert!(
        output.status.success(),
        "manual prediction failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("printf before; echo after"), "{stdout}");
}

#[test]
fn manual_prediction_decodes_extended_history_metadata() {
    let home = isolated_home();
    std::fs::write(
        home.join(".zsh_history"),
        concat!(
            ": 1720000000:1;git status\n",
            ": 1720000001:1;cargo test --workspace\n",
            ": 1720000002:1;echo break\n",
            ": 1720000003:1;git diff\n",
            ": 1720000004:1;cargo test --workspace\n",
            ": 1720000005:1;git show\n",
        ),
    )
    .expect("write extended Zsh history");

    let output = Command::new(env!("CARGO_BIN_EXE_soon"))
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .args(["--shell", "zsh", "--ngram", "1", "now"])
        .output()
        .expect("run manual prediction");
    let stdout = String::from_utf8(output.stdout).expect("UTF-8 prediction");

    let _ = std::fs::remove_dir_all(&home);
    assert!(
        output.status.success(),
        "manual prediction failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("cargo test --workspace"), "{stdout}");
    assert!(!stdout.contains("1720000001:1"), "{stdout}");
}

#[test]
fn debug_output_does_not_print_sensitive_history_text() {
    let home = isolated_home();
    let secret = "debug-secret-value";
    std::fs::write(
        home.join(".zsh_history"),
        format!("cargo test\nexport API_TOKEN={secret}\n"),
    )
    .expect("write Zsh history");

    let output = Command::new(env!("CARGO_BIN_EXE_soon"))
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .args(["--shell", "zsh", "--debug", "now"])
        .output()
        .expect("run debug prediction");
    let stdout = String::from_utf8(output.stdout).expect("UTF-8 debug output");

    let _ = std::fs::remove_dir_all(&home);
    assert!(
        output.status.success(),
        "debug prediction failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!stdout.contains(secret), "debug output leaked a secret");
    assert!(!stdout.contains("export API_TOKEN="), "{stdout}");
}
