use crate::cli::InitShell;
use crate::shell;

pub fn run(shell: InitShell) {
    match shell {
        InitShell::Zsh => print!("{}", shell::zsh::integration_script()),
    }
}
