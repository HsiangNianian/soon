#!/usr/bin/env zsh

emulate -LR zsh
setopt err_return pipe_fail

local repo_root=${0:A:h:h}
local test_root=${TMPDIR:-/tmp}/soon-release-smoke-$$
local install_root=$test_root/install
local home=$test_root/home
local project_dir=$test_root/project
local integration_file=$test_root/soon.zsh
local soon_bin=$install_root/bin/soon
local expected_version=$(awk -F '"' '/^version = / { print $2; exit }' $repo_root/Cargo.toml)
local release_cargo_home=${CARGO_HOME:-$HOME/.cargo}
local release_rustup_home=${RUSTUP_HOME:-$HOME/.rustup}
local build_target=${SOON_RELEASE_SMOKE_TARGET_DIR:-$repo_root/target/release-smoke}

function cleanup() {
  rm -rf $test_root
}
trap cleanup EXIT INT TERM HUP

mkdir -p $home $project_dir

HOME=$home \
CARGO_HOME=$release_cargo_home \
RUSTUP_HOME=$release_rustup_home \
XDG_CONFIG_HOME=$home/.config \
XDG_DATA_HOME=$home/.local/share \
CARGO_TARGET_DIR=$build_target \
  cargo install --path $repo_root --root $install_root --locked --force

[[ -x $soon_bin ]] || {
  print -u2 -- 'cargo install did not create the soon binary'
  return 1
}

local version_output=$($soon_bin --version)
[[ $version_output == "soon $expected_version" ]] || {
  print -u2 -- "unexpected installed version: $version_output"
  return 1
}

$soon_bin init zsh > $integration_file
[[ -s $integration_file ]] || {
  print -u2 -- 'soon init zsh produced an empty integration'
  return 1
}

print -rl -- \
  'git status' \
  'cargo test' \
  'git diff' \
  'printf release-smoke' \
  'git status' \
  'cargo test' \
  'git diff' > $home/.zsh_history

local manual_output
manual_output=$(HOME=$home SHELL=/bin/zsh $soon_bin --shell zsh)
[[ $manual_output == *'printf release-smoke'* ]] || {
  print -u2 -- "manual soon did not predict the fixture command: ${(qqq)manual_output}"
  return 1
}

local -a common_event_args=(
  --cwd $project_dir
  --started-at-ms 1000
  --duration-ms 25
  --shell zsh
)

HOME=$home XDG_CONFIG_HOME=$home/.config XDG_DATA_HOME=$home/.local/share \
  $soon_bin events record-command \
  --id failed-1 --command 'cargo test' --exit-code 1 $common_event_args
HOME=$home XDG_CONFIG_HOME=$home/.config XDG_DATA_HOME=$home/.local/share \
  $soon_bin events record-command \
  --id repair-1 --command 'cargo test -- --nocapture' --exit-code 0 \
  --previous-id failed-1 $common_event_args
HOME=$home XDG_CONFIG_HOME=$home/.config XDG_DATA_HOME=$home/.local/share \
  $soon_bin events record-command \
  --id success-1 --command 'cargo test' --exit-code 0 \
  --previous-id repair-1 $common_event_args
HOME=$home XDG_CONFIG_HOME=$home/.config XDG_DATA_HOME=$home/.local/share \
  $soon_bin events record-command \
  --id next-1 --command 'git status --short' --exit-code 0 \
  --previous-id success-1 $common_event_args

local repair_prediction
repair_prediction=$(HOME=$home XDG_CONFIG_HOME=$home/.config XDG_DATA_HOME=$home/.local/share \
  $soon_bin --shell zsh now --raw --after 'cargo test' --exit-code 1 --cwd $project_dir)
[[ $repair_prediction == 'cargo test -- --nocapture' ]] || {
  print -u2 -- "unexpected Repair prediction: ${(qqq)repair_prediction}"
  return 1
}

local next_prediction
next_prediction=$(HOME=$home XDG_CONFIG_HOME=$home/.config XDG_DATA_HOME=$home/.local/share \
  $soon_bin --shell zsh now --raw --after 'cargo test' --exit-code 0 --cwd $project_dir)
[[ $next_prediction == 'git status --short' ]] || {
  print -u2 -- "unexpected Next-step prediction: ${(qqq)next_prediction}"
  return 1
}

local inspect_output
inspect_output=$(HOME=$home XDG_CONFIG_HOME=$home/.config XDG_DATA_HOME=$home/.local/share \
  $soon_bin events inspect)
[[ $inspect_output == *'Command events: 4'* ]] || {
  print -u2 -- "inspect did not report seeded events: ${(qqq)inspect_output}"
  return 1
}

HOME=$home XDG_CONFIG_HOME=$home/.config XDG_DATA_HOME=$home/.local/share \
  $soon_bin events clear --yes >/dev/null
inspect_output=$(HOME=$home XDG_CONFIG_HOME=$home/.config XDG_DATA_HOME=$home/.local/share \
  $soon_bin events inspect)
[[ $inspect_output == *'Command events: 0'* ]] || {
  print -u2 -- "clear did not empty the event store: ${(qqq)inspect_output}"
  return 1
}

zsh $repo_root/tests/zsh_integration.zsh $soon_bin

cargo uninstall --root $install_root soon
[[ ! -e $soon_bin ]] || {
  print -u2 -- 'cargo uninstall left the soon binary installed'
  return 1
}

print -- 'Release install-to-uninstall smoke passed'
