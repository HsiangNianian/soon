if [[ -o interactive ]]; then
  zmodload zsh/zle
  zmodload zsh/datetime
  autoload -Uz add-zle-hook-widget
  autoload -Uz add-zsh-hook

  if (( $+functions[_soon_teardown] )); then
    _soon_teardown
  fi

  typeset -g _soon_suggestion=''
  typeset -g _soon_highlight=''
  typeset -gi _soon_fd=-1
  typeset -g SOON_LAST_LATENCY_MS=''
  typeset -g _soon_running_command=''
  typeset -g _soon_running_cwd=''
  typeset -gF _soon_running_started=0
  typeset -g _soon_running_event_id=''
  typeset -g _soon_previous_event_id=''
  typeset -g _soon_prediction_trigger='manual'
  typeset -g _soon_prediction_event_id=''
  typeset -g _soon_suggestion_id=''
  typeset -g _soon_suggestion_trigger=''
  typeset -g _soon_suggestion_event_id=''
  typeset -g _soon_suggestion_latency=''
  typeset -g _soon_accepted_command=''
  typeset -g _soon_accepted_id=''
  typeset -g _soon_accepted_trigger=''
  typeset -g _soon_accepted_event_id=''
  typeset -g _soon_accepted_latency=''
  typeset -g _soon_previous_ctrl_f=${${(z)$(bindkey '^F' 2>/dev/null)}[-1]}
  [[ -n $_soon_previous_ctrl_f ]] || _soon_previous_ctrl_f='.forward-char'

  function _soon_remove_highlight() {
    local entry
    local -a kept=()
    local -i removed=0
    for entry in "${region_highlight[@]}"; do
      if (( ! removed )) && [[ -n $_soon_highlight && $entry == $_soon_highlight ]]; then
        removed=1
      else
        kept+=("$entry")
      fi
    done
    region_highlight=("${kept[@]}")
    _soon_highlight=''
  }

  function _soon_refresh_display() {
    _soon_remove_highlight
    POSTDISPLAY=''

    if [[ -z $BUFFER && -n $_soon_suggestion ]]; then
      POSTDISPLAY=$_soon_suggestion
      _soon_highlight="0 ${#POSTDISPLAY} fg=8"
      region_highlight+=("$_soon_highlight")
    fi
  }

  function _soon_apply_suggestion() {
    _soon_refresh_display
    zle -R
  }

  function _soon_cancel_prediction() {
    if (( _soon_fd >= 0 )); then
      zle -F $_soon_fd 2>/dev/null || true
      exec {_soon_fd}<&-
      _soon_fd=-1
    fi
  }

  function _soon_prediction_ready() {
    local fd=$1
    local line=''

    if IFS= read -r line <&$fd && [[ $line == *$'\t'* ]]; then
      SOON_LAST_LATENCY_MS=${line%%$'\t'*}
      _soon_suggestion=${line#*$'\t'}
      _soon_suggestion_id="${EPOCHREALTIME//./}-$$-$RANDOM"
      _soon_suggestion_trigger=$_soon_prediction_trigger
      _soon_suggestion_event_id=$_soon_prediction_event_id
      _soon_suggestion_latency=$SOON_LAST_LATENCY_MS
      _soon_record_suggestion shown
    else
      _soon_suggestion=''
      _soon_suggestion_id=''
    fi
    zle -F $fd 2>/dev/null || true
    (( fd == _soon_fd )) && _soon_fd=-1
    exec {fd}<&-
    zle _soon_apply_suggestion
  }

  function _soon_record_suggestion() {
    local outcome=$1
    [[ -n $_soon_suggestion_id && -n $_soon_suggestion ]] || return 0

    _soon_emit_suggestion "$outcome" "$_soon_suggestion_id" "$_soon_suggestion_trigger" "$_soon_suggestion_event_id" "$_soon_suggestion_latency" "$_soon_suggestion"
  }

  function _soon_emit_suggestion() {
    local outcome=$1
    local suggestion_id=$2
    local trigger=$3
    local command_event_id=$4
    local latency=$5
    local command_text=$6

    local -a event_args=(
      events record-suggestion
      --id "$suggestion_id"
      --trigger "$trigger"
      --candidate-source history
      --command "$command_text"
      --outcome "$outcome"
      --latency-ms "$latency"
    )
    [[ -n $command_event_id ]] && event_args+=(--command-event-id "$command_event_id")
    command soon "${event_args[@]}" >/dev/null 2>&1 &!
  }

  function _soon_start_prediction() {
    local after=${1:-}
    local exit_status=${2:-}
    local command_cwd=${3:-}
    local event_id=${4:-}
    local previous_event_id=${5:-}
    local started_at_ms=${6:-}
    local duration_ms=${7:-}
    local started=$EPOCHREALTIME
    _soon_cancel_prediction
    _soon_suggestion=''

    if [[ -n $after ]]; then
      if (( exit_status == 0 )); then
        _soon_prediction_trigger='next-step'
      else
        _soon_prediction_trigger='repair'
      fi
      _soon_prediction_event_id=$event_id
      exec {_soon_fd}< <(
        local suggestion elapsed
        local -a record_args=(
          events record-command
          --id "$event_id"
          --command "$after"
          --cwd "$command_cwd"
          --started-at-ms "$started_at_ms"
          --duration-ms "$duration_ms"
          --exit-code "$exit_status"
          --shell zsh
        )
        [[ -n $previous_event_id ]] && record_args+=(--previous-id "$previous_event_id")
        command soon "${record_args[@]}" >/dev/null 2>&1
        suggestion=$(command soon --shell zsh --ngram 1 now --raw --after "$after" --exit-code "$exit_status" --cwd "$command_cwd" 2>/dev/null)
        elapsed=$(( (EPOCHREALTIME - started) * 1000.0 ))
        printf '%.3f\t%s\n' $elapsed "$suggestion"
      )
    else
      _soon_prediction_trigger='manual'
      _soon_prediction_event_id=''
      exec {_soon_fd}< <(
        local suggestion elapsed
        suggestion=$(command soon --shell zsh now --raw 2>/dev/null)
        elapsed=$(( (EPOCHREALTIME - started) * 1000.0 ))
        printf '%.3f\t%s\n' $elapsed "$suggestion"
      )
    fi

    zle -F $_soon_fd _soon_prediction_ready
  }

  function _soon_line_init() {
    (( _soon_fd >= 0 )) || _soon_start_prediction
    _soon_refresh_display
    return 0
  }

  function _soon_line_pre_redraw() {
    _soon_refresh_display
    return 0
  }

  function _soon_preexec() {
    if [[ -n $_soon_suggestion_id && -n $_soon_suggestion ]]; then
      _soon_record_suggestion dismissed
    fi
    if [[ -n $_soon_accepted_command && $1 == $_soon_accepted_command ]]; then
      _soon_emit_suggestion executed "$_soon_accepted_id" "$_soon_accepted_trigger" "$_soon_accepted_event_id" "$_soon_accepted_latency" "$_soon_accepted_command"
    fi
    _soon_accepted_command=''
    _soon_accepted_id=''
    _soon_accepted_trigger=''
    _soon_accepted_event_id=''
    _soon_accepted_latency=''
    _soon_cancel_prediction
    _soon_suggestion=''
    _soon_suggestion_id=''
    _soon_suggestion_trigger=''
    _soon_suggestion_event_id=''
    _soon_suggestion_latency=''
    _soon_refresh_display
    local -a command_words=(${(z)1})
    if [[ ${command_words[1]:-} == soon ]] ||
       [[ ${command_words[1]:-} == command && ${command_words[2]:-} == soon ]]; then
      _soon_running_command=''
      return 0
    fi
    _soon_running_command=$1
    _soon_running_cwd=$PWD
    _soon_running_started=$EPOCHREALTIME
    _soon_running_event_id="${EPOCHREALTIME//./}-$$-$RANDOM"
    return 0
  }

  function _soon_precmd() {
    local exit_status=$?

    if [[ -n $_soon_running_command ]]; then
      local command=$_soon_running_command
      local command_cwd=$_soon_running_cwd
      local event_id=$_soon_running_event_id
      local previous_event_id=$_soon_previous_event_id
      local -i started_at_ms=$(( _soon_running_started * 1000.0 ))
      local -i duration_ms=$(( (EPOCHREALTIME - _soon_running_started) * 1000.0 ))

      _soon_running_command=''
      _soon_running_cwd=''
      _soon_running_started=0
      _soon_running_event_id=''
      _soon_previous_event_id=$event_id

      _soon_start_prediction "$command" "$exit_status" "$command_cwd" "$event_id" "$previous_event_id" "$started_at_ms" "$duration_ms"
    fi

    return $exit_status
  }

  function _soon_accept() {
    if [[ -z $BUFFER && -n $_soon_suggestion ]]; then
      _soon_record_suggestion accepted
      _soon_accepted_command=$_soon_suggestion
      _soon_accepted_id=$_soon_suggestion_id
      _soon_accepted_trigger=$_soon_suggestion_trigger
      _soon_accepted_event_id=$_soon_suggestion_event_id
      _soon_accepted_latency=$_soon_suggestion_latency
      BUFFER=$_soon_suggestion
      CURSOR=${#BUFFER}
      _soon_suggestion=''
      _soon_refresh_display
    else
      zle "$_soon_previous_ctrl_f"
    fi
  }

  function _soon_teardown() {
    _soon_cancel_prediction
    add-zle-hook-widget -d line-init _soon_line_init 2>/dev/null || true
    add-zle-hook-widget -d line-pre-redraw _soon_line_pre_redraw 2>/dev/null || true
    add-zsh-hook -d preexec _soon_preexec 2>/dev/null || true
    add-zsh-hook -d precmd _soon_precmd 2>/dev/null || true
    bindkey '^F' "$_soon_previous_ctrl_f"
    zle -D _soon_accept 2>/dev/null || true
    zle -D _soon_apply_suggestion 2>/dev/null || true
    _soon_suggestion=''
    _soon_highlight=''
  }

  function soon-disable() {
    _soon_teardown
  }

  zle -N _soon_accept
  zle -N _soon_apply_suggestion
  add-zle-hook-widget line-init _soon_line_init
  add-zle-hook-widget line-pre-redraw _soon_line_pre_redraw
  add-zsh-hook preexec _soon_preexec
  add-zsh-hook precmd _soon_precmd
  bindkey '^F' _soon_accept
fi
