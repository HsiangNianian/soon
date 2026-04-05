use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

use crate::predict::main_cmd;
use crate::shell::{self, ShellKind};

pub fn get_cache_path() -> PathBuf {
    dirs::home_dir().unwrap().join(".soon_cache")
}

pub fn read_soon_cache(ngram: usize) -> Vec<String> {
    let path = get_cache_path();
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let mut cmds: Vec<String> = content
        .lines()
        .filter_map(|l| {
            let cmd = main_cmd(l).to_string();
            if cmd.is_empty() {
                None
            } else {
                Some(cmd)
            }
        })
        .collect();

    cmds.dedup();

    let n = ngram.max(1);
    if cmds.len() > n {
        cmds[cmds.len() - n..].to_vec()
    } else {
        cmds
    }
}

pub fn overwrite_soon_cache_from_history(shell: &ShellKind, cache_size: usize) {
    let history = shell::load_history(shell);
    let mut main_cmds: Vec<String> = history
        .iter()
        .map(|h| main_cmd(&h.cmd).to_string())
        .collect();
    main_cmds.dedup();
    let n = cache_size.max(1);
    let len = main_cmds.len();
    let start = len.saturating_sub(n);
    let latest_cmds = &main_cmds[start..];

    let path = get_cache_path();
    let mut file = match OpenOptions::new()
        .write(true)
        .truncate(true)
        .create(true)
        .open(&path)
    {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Warning: Failed to open cache file for overwrite: {}", e);
            return;
        }
    };

    for cmd in latest_cmds {
        if let Err(e) = writeln!(file, "{}", cmd) {
            eprintln!("Warning: Failed to write to cache: {}", e);
        }
    }
}
