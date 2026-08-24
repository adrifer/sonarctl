# sonarctl — development tasks
#
# Development happens on Linux/WSL; the runtime is a native Windows binary.

set shell := ["bash", "-euo", "pipefail", "-c"]
set positional-arguments := true

win_target := "x86_64-pc-windows-gnu"
win_install_dir := env_var_or_default("SONARCTL_WIN_DIR", "/mnt/c/Tools/sonarctl")
win_exe := "target" / win_target / "release" / "sonarctl.exe"
wrapper := env_var_or_default("SONARCTL_WRAPPER", env_var("HOME") + "/.local/bin/sonarctl")

# List the available recipes
default:
    @just --list

# Run the test suite (never requires SteelSeries GG or Sonar)
test:
    cargo test

# Format the source tree
fmt:
    cargo fmt

# Check formatting and lints
lint:
    cargo fmt --check
    cargo clippy --all-targets -- -D warnings

# Cross-compile the Windows release executable
build:
    cargo build --release --target {{win_target}}
    @echo "built {{win_exe}}"

# Opt-in tests against a locally running SteelSeries Sonar
test-sonar:
    cargo test --features sonar-integration -- --include-ignored

# Test, cross-compile and install to Windows plus the WSL wrapper
install: test build
    mkdir -p "{{win_install_dir}}"
    install -m 0755 "{{win_exe}}" "{{win_install_dir}}/sonarctl.exe"
    just _wrapper
    @echo "installed {{win_install_dir}}/sonarctl.exe"

# (Re)create the WSL wrapper that launches the Windows executable
_wrapper:
    mkdir -p "$(dirname "{{wrapper}}")"
    printf '#!/usr/bin/env bash\nexec "%s/sonarctl.exe" "$@"\n' "{{win_install_dir}}" > "{{wrapper}}"
    chmod +x "{{wrapper}}"

# Build, install and run the Windows executable with the given arguments
dev *ARGS: build
    mkdir -p "{{win_install_dir}}"
    install -m 0755 "{{win_exe}}" "{{win_install_dir}}/sonarctl.exe"
    just _wrapper
    "{{win_install_dir}}/sonarctl.exe" "$@"

# Run the installed Windows executable
run *ARGS:
    "{{win_install_dir}}/sonarctl.exe" "$@"

# Remove build artifacts
clean:
    cargo clean
