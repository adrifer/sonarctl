# sonarctl

A small CLI and TUI for controlling **SteelSeries Sonar** device routing, application routing,
volume, and mute state from a terminal.

```bash
sonarctl status
sonarctl devices
sonarctl set game headphones
sonarctl app set Discord.exe chat
sonarctl volume game 75
sonarctl mute chat toggle
sonarctl
```

## What sonarctl is

`sonarctl` is a lightweight client for Sonar's local HTTP API. It inspects and changes which
physical playback/capture device each Sonar virtual channel (Game, Chat, Media, Aux, Microphone)
is routed to, routes current Windows application audio sessions among output channels, and
controls classic-mode channel volume and mute state — without opening the SteelSeries GG window.

## What sonarctl is not

It is **not** an audio driver, a virtual audio device, or a replacement for the Sonar audio engine.
SteelSeries GG and Sonar remain installed and running; `sonarctl` only controls Sonar. It does not
touch EQ, ChatMix, spatial audio, or Windows default devices.

## Requirements

```text
Windows
SteelSeries GG
Sonar enabled and running
```

WSL is optional, and fully supported as an invocation and development environment.

## Install

### Windows

Install the latest release for your user account from PowerShell:

```powershell
$script = "$env:TEMP\install-sonarctl.ps1"
Invoke-WebRequest https://raw.githubusercontent.com/adrifer/sonarctl/main/install.ps1 -OutFile $script
& $script
Remove-Item $script
```

The installer verifies the release checksum, places `sonarctl.exe` in
`%LOCALAPPDATA%\Programs\sonarctl`, and adds that directory to your user `PATH`. It does not need
administrator access. Pass a tag such as `-Version v1.2.3` to install a specific release, or use
`-NoPath` to leave your `PATH` unchanged.

WinGet installation will be available after its community manifest is accepted:

```powershell
winget install Adrifer.Sonarctl
```

You can also download `sonarctl.exe` and `sonarctl.exe.sha256` directly from
[GitHub Releases](https://github.com/adrifer/sonarctl/releases).

### NixOS/WSL

From WSL, `just install` cross-compiles and installs everything (see
[Development](#development)).

## Quick start

```bash
sonarctl doctor            # check the SteelSeries GG / Sonar connection
sonarctl status            # show the current routing
sonarctl devices           # list playback and capture devices
sonarctl apps              # list current application audio sessions
sonarctl get game          # print the device the Game channel uses
sonarctl set game headphones
sonarctl app set Discord.exe chat
sonarctl app set --pid 1234 media
sonarctl volume             # show mixer volume and mute state
sonarctl volume game -5    # lower Game by 5 percentage points
sonarctl mute chat toggle
sonarctl                   # interactive TUI
```

Example:

```text
$ sonarctl status
Sonar: running

CHANNEL     DEVICE
Game        Arctis Nova Pro Wireless
Chat        Arctis Nova Pro Wireless
Media       LG TV
Aux         LG TV
Microphone  Shure MV7

$ sonarctl set game "LG TV"
Game → LG TV
```

## Commands

| Command | Description |
| --- | --- |
| `sonarctl` | Open the TUI (same as `sonarctl tui`) |
| `sonarctl status [--json]` | Current device for every channel |
| `sonarctl devices [--playback\|--capture] [--json]` | Physical audio devices known to Sonar |
| `sonarctl apps [--json]` | Current Windows application audio sessions and output channels |
| `sonarctl app set <name> <channel> [--json]` | Route an application by executable/display name |
| `sonarctl app set --pid <pid> <channel> [--json]` | Route one exact current process |
| `sonarctl get <channel> [--json]` | Device used by one channel |
| `sonarctl set <channel[,channel…]> <device>` | Route one or more channels |
| `sonarctl set <channel> --id "<device-id>"` | Route using an exact device id |
| `sonarctl volume [channel] [percent] [--json]` | Show or set volume; signed values are relative |
| `sonarctl mute <channel[,channel…]\|all> [mute\|toggle] [--json]` | Mute or toggle channels |
| `sonarctl unmute <channel[,channel…]\|all> [--json]` | Unmute channels |
| `sonarctl doctor [-v]` | Diagnose discovery and API problems |
| `sonarctl config path\|show` | Configuration location and contents |

Channels are `game` (alias `gaming`), `chat`, `media`, `aux` and `microphone` (alias `mic`).
Mixer commands also accept `master`; multi-channel mute commands and `volume all` accept `all`.
Volume is expressed as `0`–`100`. A leading `+` or `-` makes the value relative, and invalid
out-of-range changes are rejected rather than clamped.

Application destinations are `game`, `chat`, `media`, and `aux`; application routing does not
apply to the Microphone channel. Name matching is case-insensitive, accepts a trailing `.exe`, and
prefers exact matches before unique substrings. Ambiguous names are never guessed: use the PID
shown by `sonarctl apps`. PIDs are transient and are deliberately not stored in configuration; if
the process exits or restarts, list sessions again and use its new PID.

Global flags:

```text
--core-props <PATH>   explicit path to SteelSeries GG coreProps.json
-v, -vv               verbose logging (operational, then HTTP/discovery details)
```

`SONARCTL_CORE_PROPS` sets the same override through the environment, and `RUST_LOG=sonarctl=debug`
is honoured for logging.

## Device matching

A device argument is resolved in this order:

1. configured alias
2. exact, case-sensitive name
3. exact, case-insensitive name
4. unique case-insensitive substring

Only devices compatible with the channel are considered: playback devices for Game/Chat/Media/Aux,
capture devices for Microphone. Sonar's own virtual endpoints are never offered. Ambiguous input is
reported instead of guessed:

```text
error: multiple devices match "speakers"

  Speakers (Realtek Audio)
  Speakers (USB DAC)

Use a more specific name.
```

## Configuration

Configuration is optional; `sonarctl` works without it. The file lives at
`%APPDATA%\sonarctl\config.toml` (override with `SONARCTL_CONFIG`) and is never created
automatically.

```toml
[devices]
headphones = "Arctis Nova Pro Wireless"
speakers = "LG TV"
tv = "LG TV"
mic = "Shure MV7"

# Aliases may also pin a stable device id, with the display name as fallback.
[devices.dac]
name = "Speakers (USB DAC)"
id = "{0.0.0.00000000}.{44444444-4444-4444-4444-444444444444}"

[tui]
refresh_interval_ms = 3000
```

An alias with an `id` uses that id while it is still valid, and falls back to matching `name`
otherwise, so aliases survive most hardware re-enumerations.

## TUI

```text
┌ [1] Output routing ────────────────┐┌ Channel details ───────────────┐
│ > All Outputs  Mixed               ││ Channel  Master                 │
│   Game         Headphones          ││ Volume   ████████████░░░  80%  │
│   Chat         Headphones          ││ Muted    No                     │
│   Media        Speakers            ││                                │
│   Aux          Speakers            ││                                │
├ [2] Input routing ─────────────────┤│                                │
│   Microphone   Shure MV7           ││                                │
├ [3] [Applications] Devices ────────┤│ Applications                   │
│ > Discord         Game    Active   ││ Discord  PID 4820              │
│   Spotify         Media   Active   ││ Spotify  PID 9012              │
│   Microsoft Edge  Media   Idle     ││                                │
└────────────────────────────────────┘└────────────────────────────────┘
 [1] Output  [2] Input  [3] Applications/Devices  │  ? help  q quit
```

`All Outputs` changes Game, Chat, Media, and Aux in one action. Microphone stays separate in the
numbered Input pane. Channel details follow the selected output route (with `All Outputs` mapped to
Master) or Microphone when Input is selected. The panel does not need focus: press `h`/`l` or
`[`/`]` while a route is selected to change its volume. Steps are 1% between 0% and 5%, then 5%
from 5% upward, and `m` toggles mute.
Press `1`, `2`, or `3` to focus a numbered pane directly; `Tab` cycles focus. Pane 3 opens on
**Applications** and also contains a **Devices** tab. While pane 3 is focused, use `h`/`l`,
`[`/`]`, or `←`/`→` to switch tabs (`a` and `d` select one directly). This is contextual: those
same bracket and Vim keys still change volume while Output or Input is focused.

| Key | Output/Input panes | Applications tab | Devices tab | Picker |
| --- | --- | --- | --- | --- |
| `1` / `2` / `3` | focus numbered pane | focus numbered pane | focus numbered pane | — |
| `Tab` / `Shift+Tab` | cycle pane focus | cycle pane focus | cycle pane focus | — |
| `j` / `↓`, `k` / `↑` | select route | select application | select device | select item |
| `g` / `G` | first / last route | first / last application | first / last device | first / last item |
| `Enter` | open device picker | open channel picker | toggle picker visibility | apply |
| `l` / `]`, `h` / `[` | change volume (1% at low levels, otherwise 5%) | switch tabs | switch tabs | — |
| `a` / `d` | — | select Applications / Devices | select Applications / Devices | — |
| `m` | toggle selected mute | — | — | — |
| `Space` | — | — | toggle picker visibility | — |
| `/` | — | — | — | filter device picker |
| `Esc` / `q` | quit | quit | quit | cancel |
| `r` | refresh | refresh | refresh | — |
| `?` | help | help | help | help |

Selecting an application changes the right pane to **Application details**; `Enter` opens a picker
for Game, Chat, Media, or Aux. When Output is selected, **Channel details** lists applications
assigned to that channel. `All Outputs` lists every routed output application with its channel;
Microphone explains that application routing is output-only.

The Devices tab groups physical playback hardware under **Output devices** and capture hardware
under **Input devices**. It controls which devices appear in route pickers. Toggled visibility is
stored by stable device ID in `%APPDATA%\sonarctl\device-visibility.toml`; it does not disable
hardware in Windows.

State refreshes every 3 seconds (configurable) and immediately after a change. The terminal is
always restored — on quit, `Ctrl+C`, errors and panics.

## JSON output

```bash
sonarctl status --json
sonarctl devices --json
sonarctl apps --json
sonarctl app set --pid 1234 media --json
sonarctl get game --json
sonarctl volume --json
sonarctl mute chat toggle --json
```

```json
{
  "channel": "game",
  "device": {
    "id": "{0.0.0.00000000}.{11111111-1111-1111-1111-111111111111}",
    "name": "Arctis Nova Pro Wireless"
  }
}
```

Output is plain text with no ANSI styling, so `DEVICE=$(sonarctl get game)` works as expected.
Application JSON uses stable `process_id`, `process_name`, `display_name`, `channel`, `activity`,
and `routing_error` fields under an `applications` array. The mutation command returns the same
schema containing the changed application.

## Exit codes

```text
0   success
1   generic failure
2   invalid CLI arguments
3   SteelSeries GG unavailable
4   Sonar unavailable
5   device not found
6   ambiguous device match
7   incompatible/unexpected Sonar API
8   configuration error
```

## Using `sonar` from WSL

WSL support works by launching the Windows binary:

```text
WSL
 │
 └─ ~/.local/bin/sonar
       │
       └─ /mnt/c/Tools/sonarctl/sonarctl.exe
```

The wrapper contains no Sonar logic — it only forwards arguments, stdin/stdout/stderr and the exit
code:

```bash
#!/usr/bin/env bash
export TERM="${TERM:-xterm-256color}"
if [[ ":${WSLENV:-}:" != *":TERM:"* && ":${WSLENV:-}:" != *":TERM/"* ]]; then
  export WSLENV="${WSLENV:+${WSLENV%:}:}TERM"
fi
exec "/mnt/c/Tools/sonarctl/sonarctl.exe" "$@"
```

> Windows executables do not need to be imported into the WSL `PATH`. The wrapper handles
> invocation explicitly. It also forwards `TERM` through WSL interop so Crossterm uses ANSI
> alternate-screen sequences and restores the shell contents when the TUI exits.

Because the binary is a native Windows process, `localhost` always means Windows and no WSL
networking, NAT or host-IP discovery logic is needed. The TUI runs fine in Windows Terminal from
both WSL and PowerShell.

## Development

Development happens on Linux/WSL; the runtime target is Windows.

```bash
rustup target add x86_64-pc-windows-gnu

just test        # cargo test — never needs SteelSeries GG or Sonar
just lint        # rustfmt check + clippy
just install     # test, cross-compile, install to C:\Tools\sonarctl + WSL wrapper
sonar doctor
```

`just install` is idempotent: it replaces `C:\Tools\sonarctl\sonarctl.exe` and rewrites the WSL
wrapper without leaving backups behind. Override the destinations with `SONARCTL_WIN_DIR` and
`SONARCTL_WRAPPER`.

Other recipes:

```bash
just build            # cargo build --release --target x86_64-pc-windows-gnu
just dev doctor       # build, install, then run with the given arguments
just run status       # run the already installed executable
just test-sonar       # opt-in tests against the real local Sonar installation
```

The MSVC target is not required. Cross-compiling needs a MinGW-w64 toolchain
(`x86_64-w64-mingw32-gcc`); `build.rs` transparently works around toolchains that do not ship
`libpthread.a` (for example MinGW builds using the `mcf` threading model).

### Project layout

```text
src/
├── main.rs          CLI entry point, exit codes, logging
├── lib.rs           module wiring
├── cli.rs           clap definitions
├── app.rs           application layer shared by CLI and TUI
├── config.rs        config.toml handling
├── doctor.rs        diagnostics
├── output.rs        text and JSON rendering
├── error.rs         errors, hints and exit codes
├── sonar/           everything SteelSeries-specific
│   ├── backend.rs   SonarBackend trait, rediscovery and retry
│   ├── client.rs    HTTP clients for GG and Sonar
│   ├── discovery.rs coreProps.json, /subApps, locality checks
│   ├── applications.rs application session parsing and routing paths
│   ├── models.rs    Channel, device, route, mixer and application models
│   └── routing.rs   channel ↔ API id mapping, URL encoding
├── platform/        Windows paths
└── tui/             ratatui interface (app state, events, rendering)
```

CLI and TUI both call the application layer; only `src/sonar/` knows about SteelSeries' API.

### Testing

`cargo test` runs entirely offline using JSON fixtures in `tests/fixtures/` and a mock HTTP server
(`wiremock`). Covered: coreProps parsing, GG/Sonar discovery, device and route parsing, virtual
device filtering, URL encoding, application-session collapsing, verified route/application/
volume/mute mutations, malformed payloads, Sonar port changes and rediscovery, configuration,
device and application matching, CLI parsing, rendered output and TUI state transitions.

Tests against a real installation are opt-in:

```bash
cargo test --features sonar-integration -- --include-ignored
```

## Security

`sonarctl` only talks to `localhost`/`127.0.0.1`/`::1`. Endpoints read from `coreProps.json` and
`/subApps` are validated before any request is made. The GG endpoint uses a local self-signed
certificate, so certificate validation is relaxed **only** for that dedicated client and **only**
after the address has been proven to be loopback; the Sonar client is separate and strict. There is
no telemetry, no analytics, no remote configuration and no internet access.

## Compatibility

> SteelSeries Sonar does not expose a documented public API for this functionality. `sonarctl`
> relies on Sonar's local internal API, which SteelSeries may change between GG releases.

Expect occasional compatibility updates. JSON parsing is deliberately tolerant (unknown fields are
ignored, optional fields tolerated), and every SteelSeries-specific mapping is isolated in
`src/sonar/`. When something breaks, `sonarctl doctor -v` shows exactly which step failed.

## License

MIT. See [LICENSE](LICENSE).
