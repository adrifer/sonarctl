#!/usr/bin/env bash

sonarctl_exe=@SONARCTL_EXE@

export TERM="${TERM:-xterm-256color}"
if [[ ":${WSLENV:-}:" != *":TERM:"* && ":${WSLENV:-}:" != *":TERM/"* ]]; then
  export WSLENV="${WSLENV:+${WSLENV%:}:}TERM"
fi

exec "$sonarctl_exe" "$@"
