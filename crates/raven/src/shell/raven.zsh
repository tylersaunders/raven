autoload -U add-zsh-hook

_raven_preexec() {
  local id
  id=$(raven history start -- "$1")
  export RAVEN_HISTORY_ID="$id"
}

_raven_precmd() {
  local EXIT="$?"
  [[ -z "${RAVEN_HISTORY_ID:-}" ]] && return

  (raven history end  --exit $EXIT -- $RAVEN_HISTORY_ID)
  # Clear the ID for the next command.
  export RAVEN_HISTORY_ID=""
}

add-zsh-hook preexec _raven_preexec
add-zsh-hook precmd _raven_precmd
