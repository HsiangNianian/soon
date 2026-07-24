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
    else
      _soon_suggestion=''
    fi
    zle -F $fd 2>/dev/null || true
    (( fd == _soon_fd )) && _soon_fd=-1
    exec {fd}<&-
    zle _soon_apply_suggestion
  }

  function _soon_start_prediction() {
    local after=${1:-}
    local started=$EPOCHREALTIME
    _soon_cancel_prediction
    _soon_suggestion=''

    if [[ -n $after ]]; then
      exec {_soon_fd}< <(
        local suggestion elapsed
        suggestion=$(command soon --shell zsh --ngram 1 now --raw --after "$after" 2>/dev/null)
        elapsed=$(( (EPOCHREALTIME - started) * 1000.0 ))
        printf '%.3f\t%s\n' $elapsed "$suggestion"
      )
    else
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
    _soon_start_prediction "$1"
    return 0
  }

  function _soon_accept() {
    if [[ -z $BUFFER && -n $_soon_suggestion ]]; then
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
  bindkey '^F' _soon_accept
fi
