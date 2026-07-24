use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

fn isolated_home() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock")
        .as_nanos();
    let path = std::env::temp_dir().join(format!("soon-zsh-test-{}-{nonce}", std::process::id()));
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
