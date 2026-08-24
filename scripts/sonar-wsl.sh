#!/usr/bin/env bash

sonarctl_exe=@SONARCTL_EXE@

export TERM="${TERM:-xterm-256color}"
if [[ ":${WSLENV:-}:" != *":TERM:"* && ":${WSLENV:-}:" != *":TERM/"* ]]; then
  export WSLENV="${WSLENV:+${WSLENV%:}:}TERM"
fi

is_tui=true
skip_next=false
for arg in "$@"; do
  if $skip_next; then
    skip_next=false
    continue
  fi
  case "$arg" in
    --core-props) skip_next=true ;;
    --core-props=* | --verbose | -v* | tui | --) ;;
    *)
      is_tui=false
      break
      ;;
  esac
done
if $skip_next; then
  is_tui=false
fi

if ! $is_tui; then
  exec "$sonarctl_exe" "$@"
fi

if [[ ":${WSLENV:-}:" != *":SONARCTL_EXTERNAL_ALT_SCREEN:"* \
  && ":${WSLENV:-}:" != *":SONARCTL_EXTERNAL_ALT_SCREEN/"* ]]; then
  export WSLENV="${WSLENV:+${WSLENV%:}:}SONARCTL_EXTERNAL_ALT_SCREEN"
fi

if ! error_log="$(mktemp)"; then
  echo "sonar: could not create a temporary error log" >&2
  exit 1
fi

restore_screen() {
  printf "\033[?1049l\033[?25h"
  if [[ -s "$error_log" ]]; then
    cat "$error_log" >&2
  fi
  rm -f "$error_log"
}
trap restore_screen EXIT

printf "\033[?1049h"
SONARCTL_EXTERNAL_ALT_SCREEN=1 "$sonarctl_exe" "$@" </dev/tty 2>"$error_log"
status=$?
exit "$status"
