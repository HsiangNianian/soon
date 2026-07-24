#!/usr/bin/env zsh

emulate -LR zsh
setopt err_return pipe_fail
zmodload zsh/datetime
zmodload zsh/zpty

local soon_bin=${1:-target/debug/soon}
if [[ ! -x $soon_bin ]]; then
  cargo build --locked
fi
soon_bin=${soon_bin:A}

local test_root=${TMPDIR:-/tmp}/soon-zsh-harness-$$
local fake_bin=$test_root/bin
local integration_file=$test_root/integration.zsh
local accepted_file=/tmp/soon-zsh-accept-$$
mkdir -p $fake_bin

function cleanup() {
  zpty -d soon_shell 2>/dev/null || true
  rm -f $accepted_file
  rm -rf $test_root
}
trap cleanup EXIT INT TERM HUP

$soon_bin init zsh > $integration_file

cat > $fake_bin/soon <<FAKE_SOON
#!/bin/sh
sleep 0.05
printf '%s\n' 'touch $accepted_file'
FAKE_SOON
chmod +x $fake_bin/soon

cat > $test_root/.zshrc <<ZSHRC
stty 38400 columns 120 rows 24 tabs -icanon -iexten
function _soon_harness_tc() { REPLY=''; }
zle -T tc _soon_harness_tc
function _soon_harness_previous_ctrl_f() {
  BUFFER+='ORIGINAL_CTRL_F'
  CURSOR=\${#BUFFER}
}
zle -N _soon_harness_previous_ctrl_f
bindkey '^F' _soon_harness_previous_ctrl_f
PS1='SOON_PROMPT> '
RPROMPT=''
source ${(q)integration_file}
ZSHRC

local shell_path=$fake_bin:$PATH
zpty soon_shell env PATH=${(q)shell_path} ZDOTDIR=${(q)test_root} HOME=${(q)test_root} TERM=xterm-256color zsh -d -i

typeset -g transcript=''

function wait_for_output() {
  local needle=$1
  local timeout=${2:-5}
  local chunk=''
  local -F deadline=$(( EPOCHREALTIME + timeout ))

  while (( EPOCHREALTIME < deadline )); do
    if zpty -r -t soon_shell chunk; then
      transcript+=$chunk
      if [[ $transcript == *$needle* ]]; then
        return 0
      fi
    fi
    sleep 0.02
  done

  print -u2 -- "timed out waiting for: $needle"
  print -u2 -- "transcript: ${(qqq)transcript}"
  return 1
}

function wait_for_file() {
  local target_file=$1
  local -F deadline=$(( EPOCHREALTIME + 5 ))
  local progress_chunk=''

  while (( EPOCHREALTIME < deadline )); do
    [[ -e $target_file ]] && return 0
    if zpty -r -t soon_shell progress_chunk; then
      transcript+=$progress_chunk
    fi
    sleep 0.02
  done

  print -u2 -- "timed out waiting for file: $target_file"
  local failure_chunk=''
  local failure_output=''
  while zpty -r -t soon_shell failure_chunk; do
    failure_output+=$failure_chunk
  done
  print -u2 -- "shell output: ${(qqq)failure_output}"
  return 1
}

# Render and accept a suggestion at an empty prompt.
wait_for_output 'SOON_PROMPT> '
transcript=''
wait_for_output "touch $accepted_file"
transcript=''
zpty -w -n soon_shell $'\x06'
sleep 0.05
zpty -w -n soon_shell $'\n'
wait_for_file $accepted_file
wait_for_output 'SOON_PROMPT> '

# Normal typing ignores the ghost suggestion instead of modifying the buffer.
rm -f $accepted_file
transcript=''
wait_for_output "touch $accepted_file"
transcript=''
zpty -w soon_shell '[[ $SOON_LAST_LATENCY_MS == <->.<-> ]] && print -r -- SOON_TYPED_NORMALLY'
wait_for_output 'SOON_TYPED_NORMALLY'
wait_for_output 'SOON_PROMPT> '
[[ ! -e $accepted_file ]] || {
  print -u2 -- 'typing a command unexpectedly accepted the suggestion'
  return 1
}

# Teardown removes hooks and cancels pending prediction work.
transcript=''
zpty -w soon_shell 'soon-disable'
wait_for_output 'SOON_PROMPT> '
transcript=''
sleep 0.2
local chunk=''
while zpty -r -t soon_shell chunk; do
  transcript+=$chunk
done
[[ $transcript != *"touch $accepted_file"* ]] || {
  print -u2 -- 'suggestion rendered after soon-disable'
  return 1
}

transcript=''
zpty -w -n soon_shell $'\x06'
wait_for_output 'ORIGINAL_CTRL_F'
zpty -w -n soon_shell $'\x15'

zpty -w soon_shell 'exit'
print -- 'Zsh integration harness passed'
