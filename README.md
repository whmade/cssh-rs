<p align="center"><a href="https://github.com/whmade/cssh-rs"><img src="https://raw.githubusercontent.com/whmade/cssh-rs/refs/heads/main/cssh-rs/res/cssh-rs.svg" width="100" alt="cssh-rs Logo" /></a></p>
<h1 align="center">cssh-rs</h1>
<p align="center"><i>Cross-platform cluster SSH tool</i></p>
<p align="center">
  <img src="https://custom-icon-badges.demolab.com/badge/Windows-0078D6?logo=windows11&logoColor=white&style=for-the-badge" alt="Windows">
  <img src="https://img.shields.io/badge/Linux-FCC624?logo=linux&logoColor=black&style=for-the-badge" alt="Linux">
  <img src="https://img.shields.io/badge/macOS-000000?logo=apple&logoColor=white&style=for-the-badge" alt="macOS">
</p>
<p align="center">
  <a href="./LICENSE.txt"><img src="https://img.shields.io/badge/License-Apache_2.0-blue.svg"></a>
  <a href="https://github.com/whmade/cssh-rs/releases/latest"><img src="https://img.shields.io/github/v/release/whmade/cssh-rs.svg"></a>
  <a href="https://github.com/whmade/cssh-rs/releases"><img src="https://img.shields.io/github/downloads/whmade/cssh-rs/total"></a><br>
  <a href="https://github.com/whmade/cssh-rs/actions/workflows/post-submit.yml"><img src="https://github.com/whmade/cssh-rs/actions/workflows/post-submit.yml/badge.svg"></a>
  <a href="https://github.com/whmade/cssh-rs/actions/workflows/deploy_github_pages.yml"><img src="https://github.com/whmade/cssh-rs/actions/workflows/deploy_github_pages.yml/badge.svg"></a>
  <!--TODO: Add link to coverage once coverage data looks better: https://github.com/insightsengineering/coverage-action/issues/28#issuecomment-1743910648 -->
</p>

![cssh-rs demo](https://raw.githubusercontent.com/whmade/cssh-rs/refs/heads/main/demo/cssh-rs.gif)[^1][^2][^3]

## Overview
cssh-rs will launch 1 daemon and N client windows (with N being the number of hosts to SSH onto).<br>
Key-strokes performed while having the daemon console focussed will be sent to all clients simoultaneously and be replayed by them.<br>
Focussing a client will cause any key-strokes to be sent to this client only.

## Cross-platform
cssh-rs is designed to run on Windows, Linux, and macOS.

<table align="center" border="0" cellspacing="0" cellpadding="0">
  <tr>
    <td>

<table>
  <thead>
    <tr><th colspan="2" align="center"><img height="60" src="https://custom-icon-badges.demolab.com/badge/Windows-0078D6?logo=windows11&logoColor=white&style=for-the-badge" alt="Windows"></th></tr>
    <tr><th>Terminal</th><th>Status</th></tr>
  </thead>
  <tbody>
    <tr><td>conhost</td><td><img src="https://img.shields.io/badge/-implemented-brightgreen" alt="implemented"></td></tr>
    <tr><td>Windows Terminal</td><td><img src="https://img.shields.io/badge/-planned-yellow" alt="planned"></td></tr>
    <tr><td>WezTerm</td><td><img src="https://img.shields.io/badge/-planned-yellow" alt="planned"></td></tr>
    <tr><td>Ghostty</td><td><img src="https://img.shields.io/badge/-planned-yellow" alt="planned"></td></tr>
  </tbody>
</table>

</td>
    <td>

<table>
  <thead>
    <tr><th colspan="2" align="center"><img height="60" src="https://img.shields.io/badge/Linux-FCC624?logo=linux&logoColor=black&style=for-the-badge" alt="Linux"></th></tr>
    <tr><th>Terminal</th><th>Status</th></tr>
  </thead>
  <tbody>
    <tr><td>GNOME Terminal</td><td><img src="https://img.shields.io/badge/-planned-yellow" alt="planned"> (X11)</td></tr>
    <tr><td>Alacritty</td><td><img src="https://img.shields.io/badge/-planned-yellow" alt="planned"> (X11, Wayland)</td></tr>
    <tr><td>Kitty</td><td><img src="https://img.shields.io/badge/-planned-yellow" alt="planned"> (X11, Wayland)</td></tr>
    <tr><td>WezTerm</td><td><img src="https://img.shields.io/badge/-planned-yellow" alt="planned"> (X11, Wayland)</td></tr>
    <tr><td>Ghostty</td><td><img src="https://img.shields.io/badge/-planned-yellow" alt="planned"> (X11, Wayland)</td></tr>
  </tbody>
</table>

</td>
    <td>

<table>
  <thead>
    <tr><th colspan="2" align="center"><img height="60" src="https://img.shields.io/badge/macOS-000000?logo=apple&logoColor=white&style=for-the-badge" alt="macOS"></th></tr>
    <tr><th>Terminal</th><th>Status</th></tr>
  </thead>
  <tbody>
    <tr><td>Alacritty</td><td><img src="https://img.shields.io/badge/-planned-yellow" alt="planned"></td></tr>
    <tr><td>Kitty</td><td><img src="https://img.shields.io/badge/-planned-yellow" alt="planned"></td></tr>
    <tr><td>WezTerm</td><td><img src="https://img.shields.io/badge/-planned-yellow" alt="planned"></td></tr>
    <tr><td>Ghostty</td><td><img src="https://img.shields.io/badge/-planned-yellow" alt="planned"></td></tr>
    <tr><td>Terminal.app</td><td><img src="https://img.shields.io/badge/-planned-yellow" alt="planned"></td></tr>
    <tr><td>iTerm2</td><td><img src="https://img.shields.io/badge/-planned-yellow" alt="planned"></td></tr>
  </tbody>
</table>

</td>
  </tr>
</table>

**Linux Wayland scope:** Wayland support targets wlroots (Sway,
Hyprland, river, Niri, COSMIC) and KWin (KDE Plasma). Mutter (GNOME on
Wayland) is ![not planned](https://img.shields.io/badge/-not%20planned%20%28v1%29-red) for v1.

## Download/Installation
cssh-rs is a portable application and is not installed.<br>
To download the cssh-rs application refer to the [Releases 📦](https://github.com/whmade/cssh-rs/releases) page.

> [!NOTE]
> The released `cssh-rs.exe` is not code-signed. On first launch Windows
> SmartScreen shows a "Windows protected your PC" warning because the
> binary was downloaded from the internet and has no recognized
> publisher. To run it: click `More info` -> `Run anyway`, or
> right-click `cssh-rs.exe` -> `Properties` -> tick `Unblock` ->
> `Apply`. Equivalent from PowerShell: `Unblock-File .\cssh-rs.exe`.
>
> The release `.zip` itself (0.19.0 and later) is signed with a
> [GitHub build attestation](https://docs.github.com/en/actions/security-for-github-actions/using-artifact-attestations/using-artifact-attestations-to-establish-provenance-for-builds);
> see the release notes for verification instructions. Verifying it
> proves the archive came from this project's release workflow at the
> tagged commit, but does not change SmartScreen's behavior for the
> unsigned `.exe` inside.

## Pre-requisite
- Any SSH client (Windows 10 and Windows 11 already include a built-in SSH server and client - [docs](https://learn.microsoft.com/en-us/windows/terminal/tutorials/ssh))

## Usage

<!-- HELP_OUTPUT_START -->
```cmd
cssh-rs.exe --help
Cross-platform cluster SSH tool

Usage: cssh-rs.exe [OPTIONS] [HOSTS]... [COMMAND]

Commands:
  client  Subcommand that will launch a single client window
  daemon  Subcommand that will launch the daemon window
  help    Print this message or the help of the given subcommand(s)

Arguments:
  [HOSTS]...
          Hosts and/or cluster tag(s) to connect to

          Hosts or cluster tags might use brace expansion, but need to be properly quoted.

          E.g.: `cssh-rs.exe "host{1..3}" hostA`

          Hosts can include a username which will take precedence over the username given via the `-u` option and over any ssh config value.

          E.g.: `cssh-rs.exe -u user3 user1@host1 userA@hostA host3`

          Hosts can include a port number which will take precedence over the port given via the `-p` option.

          E.g.: `cssh-rs.exe -p 33 host1:11 host2:22 host3`

          If no hosts are provided and the application is launched in a new console window (e.g. by double clicking the executable in the File Explorer), it will launch in interactive mode.

Options:
  -u, --username <USERNAME>
          Optional username used to connect to the hosts

  -p, --port <PORT>
          Optional port used for all SSH connections

  -d, --debug
          Enable extensive logging

  -h, --help
          Print help (see a summary with '-h')

  -V, --version
          Print version
```
<!-- HELP_OUTPUT_END -->
Example:
`cssh-rs.exe -u root hosta.dev hostb.dev hostc.dev`

We recommend using the [ssh_config](https://linux.die.net/man/5/ssh_config) for any configurations like default username etc.

## Configuration

`cssh-rs-config.toml` contains all relevant configurations and is located in the same directory as the executable.
It is automatically created with default values if not present.

### `clusters`
An array of clusters that can be used to alias a set of host names to a specific tag:
```toml
clusters = [
    { name = "dev", hosts = ["hosta.dev", "root@hostb.dev", "hostc.dev"] }
]
```
Clusters may be nested, but be aware of recursive clusters which are not checked for.

### `[client]`
A collection containing client relevant configuration.

```toml
[client]
ssh_config_path = 'C:\Users\demo_user\.ssh\config'
program = 'ssh'
arguments = ['-XY', '{{USERNAME_AT_HOST}}']
username_host_placeholder = '{{USERNAME_AT_HOST}}'
disabled_console_color = 135
highlighted_console_color = 31
```

| Option | Type | Default | Description |
|---|---|---|---|
| `ssh_config_path` | path | auto-detected | Full qualified path to your ssh config file. |
| `program` | string | `'ssh'` | Executable used to establish ssh connections. |
| `arguments` | list of strings | `['-XY', '{{USERNAME_AT_HOST}}']` | Additional arguments passed to `program`. |
| `username_host_placeholder` | string | `'{{USERNAME_AT_HOST}}'` | Token in `arguments` replaced with `username@host`. |
| <a id="disabled_console_color"></a>`disabled_console_color` | u16 | `135` (`4+2+1+128`) | Colors used while a client is in the disabled state (input ignored). Default paints default-grey text on muted dark-grey. See [Console color encoding](#console-color-encoding). |
| <a id="highlighted_console_color"></a>`highlighted_console_color` | u16 | `31` (`4+2+1+8+16`) | Colors used while a client is the currently selected window in the control-mode `[e]nable/disable input` submenu. Default paints bright-white text on blue. See [Console color encoding](#console-color-encoding) and [Highlight overlay](#highlight-overlay). |

### `[daemon]`
A collection containing daemon relevant configuration.

```toml
[daemon]
height = 200
aspect_ratio_adjustment = -1.0
console_color = 207
submenu_edge_behavior = 'clamp'
```

| Option | Type | Default | Description |
|---|---|---|---|
| `height` | u16 | `200` | Height of the daemon console. |
| `aspect_ratio_adjustment` | float | `-1.0` | Bias for vertical vs. horizontal layout of client windows. See [Aspect ratio](#aspect-ratio). |
| <a id="console_color"></a>`console_color` | u16 | `207` (`8+4+2+1+64+128`) | Daemon console colors. Default paints white text on red. See [Console color encoding](#console-color-encoding). |
| `submenu_edge_behavior` | enum | `'clamp'` | What happens when an arrow / `hjkl` keystroke in the submenu would move the highlight past the edge of the client grid. See [Submenu edge behavior](#submenu-edge-behavior). |

### Aspect ratio
`aspect_ratio_adjustment` configures whether the available screen space should rather be used horizontally or vertically.
* `> 0.0` - aims for vertical rectangle shape. The larger the value, the more exaggerated the "verticality". Eventually the windows will all be columns.
* `= 0.0` - aims for square shape.
* `< 0.0` - aims for horizontal rectangle shape. The smaller the value, the more exaggerated the "horizontality". Eventually the windows will all be rows. `-1.0` is the sweetspot for mostly preserving a 16:9 ratio.

### Console color encoding
Console-color options encode background and foreground attributes as a single integer.
Available are all standard Windows color combinations ([windows docs](https://learn.microsoft.com/en-us/windows/console/console-screen-buffers#character-attributes)):
```
FOREGROUND_BLUE:        1
FOREGROUND_GREEN:       2
FOREGROUND_RED:         4
FOREGROUND_INTENSITY:   8
BACKGROUND_BLUE:        16
BACKGROUND_GREEN:       32
BACKGROUND_RED:         64
BACKGROUND_INTENSITY:   128
```

### Highlight overlay
The highlight wins over [`disabled_console_color`](#disabled_console_color); pressing `[d]`/`[e]`/`[t]` on the selected window briefly flashes the underlying state color (~250ms) as action feedback before the highlight color is restored.

### Submenu edge behavior
`submenu_edge_behavior` selects what happens when an arrow / `hjkl` keystroke in the `[e]nable/disable input` submenu would move the highlight past the edge of the client grid:
* `'clamp'` (default) - the highlight stays on the current cell.
* `'wrap'` - the highlight wraps to the opposite edge of the same row (Left/Right) or column (Up/Down).

## Contributing
cssh-rs uses pre-commit githooks to enforce good code style.<br>
Install them via ``git config --local core.hooksPath .githooks/``.

## Releases
Step by step guide to create a new release:
- `cargo xtask prepare-release` and follow the instructions
- Create a pull request from the new maintenance branch to main OR cherry-pick the new Version change from the existing maintenance branch to main
- `cargo xtask create-release-tag` and follow the instructions
- Revise the automatically created Release Draft and publish it

[^1]: The searchbar used to launch cssh-rs in the demo clip is [keypirinha](https://keypirinha.com/).
[^2]: The tool to show key presses in the demo clip is [carnac the magnificent](https://github.com/Code52/carnac).
[^3]: The tool used to record the screen as GIF is [ScreenToGif](https://github.com/NickeManarin/ScreenToGif).
