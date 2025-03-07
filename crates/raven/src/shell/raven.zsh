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

_raven_search_history_up() {
    # Only trigger if the buffer is a single line
    if [[ ! $BUFFER == *$'\n'* ]]; then
        _raven_search_history --shell-up-key "$@"
    else
        zle up-line
    fi
}

_raven_search_history() {

  # Emualte zsh in local mode
  emulate -L zsh
  zle -I

  local output
  output=$(RAVEN_QUERY=$BUFFER raven search $* --interactive)

  zle reset-prompt

  if [[ -n $output ]]; then
    RBUFFER=""
    LBUFFER=$output
  fi

}

zle -N raven-search-history _raven_search_history
zle -N raven-search-history-up _raven_search_history_up

add-zsh-hook preexec _raven_preexec
add-zsh-hook precmd _raven_precmd
