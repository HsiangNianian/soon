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
local repair_file=/tmp/soon-zsh-repair-$$
local calls_file=$test_root/soon-calls.log
mkdir -p $fake_bin

function cleanup() {
  zpty -d soon_shell 2>/dev/null || true
  rm -f $accepted_file $repair_file
  rm -rf $test_root
}
trap cleanup EXIT INT TERM HUP

$soon_bin init zsh > $integration_file

cat > $fake_bin/soon <<FAKE_SOON
#!/bin/sh
printf '%s\n' "\$*" >> '$calls_file'
case " \$* " in
  *' events record-command '*|*' events record-suggestion '*) exit 0 ;;
esac
sleep 0.05
case " \$* " in
  *' --exit-code 0 '*) printf '%s\n' 'touch $accepted_file' ;;
  *' --exit-code '*) printf '%s\n' 'touch $repair_file' ;;
  *) printf '%s\n' 'touch $accepted_file' ;;
esac
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
    if [[ $transcript == *"$needle"* ]]; then
      transcript=${transcript#*"$needle"}
      return 0
    fi
    if zpty -r -t soon_shell chunk; then
      transcript+=$chunk
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

function wait_for_log() {
  local needle=$1
  local -F deadline=$(( EPOCHREALTIME + 5 ))

  while (( EPOCHREALTIME < deadline )); do
    if [[ -f $calls_file ]] && [[ "$(<$calls_file)" == *$needle* ]]; then
      return 0
    fi
    sleep 0.02
  done

  print -u2 -- "timed out waiting for log entry: $needle"
  [[ -f $calls_file ]] && print -u2 -- "soon calls: ${(qqq)$(<$calls_file)}"
  return 1
}

# Render and accept a suggestion at an empty prompt.
wait_for_output 'SOON_PROMPT> '
wait_for_output "touch $accepted_file"
wait_for_log '--outcome shown'
zpty -w -n soon_shell $'\x06'
wait_for_log '--outcome accepted'
sleep 0.05
zpty -w -n soon_shell $'\n'
wait_for_file $accepted_file
wait_for_log '--outcome executed'
wait_for_output 'SOON_PROMPT> '

# Normal typing ignores the ghost suggestion instead of modifying the buffer.
rm -f $accepted_file
wait_for_output "touch $accepted_file"
zpty -w soon_shell '[[ $SOON_LAST_LATENCY_MS == <->.<-> ]] && print -r -- SOON_TYPED_NORMALLY'
wait_for_output 'SOON_TYPED_NORMALLY'
wait_for_output 'SOON_PROMPT> '
wait_for_log '--outcome dismissed'
[[ ! -e $accepted_file ]] || {
  print -u2 -- 'typing a command unexpectedly accepted the suggestion'
  return 1
}

# Completed commands drive distinct Next-step and Repair predictions, while preserving `$?`.
zpty -w soon_shell 'true'
wait_for_output 'SOON_PROMPT> '
wait_for_log 'record-command --id'
wait_for_log '--command true'
wait_for_log '--exit-code 0'

zpty -w soon_shell 'false'
wait_for_output 'SOON_PROMPT> '
wait_for_output "touch $repair_file"
wait_for_log '--command false'
wait_for_log '--exit-code 1'

zpty -w soon_shell 'print -r -- "SOON_STATUS:$?"'
wait_for_output 'SOON_STATUS:1'
wait_for_output 'SOON_PROMPT> '

# Invoking soon is a manual prediction trigger, not a command event to learn.
zpty -w soon_shell 'soon'
wait_for_output 'SOON_PROMPT> '
sleep 0.1
[[ "$(<$calls_file)" != *'--command soon --cwd'* ]] || {
  print -u2 -- 'manual soon invocation was recorded as a command event'
  return 1
}

# Teardown removes hooks and cancels pending prediction work.
zpty -w soon_shell 'soon-disable'
wait_for_output 'SOON_PROMPT> '
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
