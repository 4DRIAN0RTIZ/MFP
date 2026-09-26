# MFP — Music For Programming Player

Lightweight Rust terminal player for [musicforprogramming.net](https://musicforprogramming.net/).

**[→ Documentation & full feature overview](https://4DRIAN0RTIZ.github.io/mfp/)**

![mfp](./mfp.png)

## Install

```bash
git clone https://github.com/4DRIAN0RTIZ/mfp && cd mfp
cargo build --release
sudo cp target/release/mfp /usr/local/bin/  # optional
```

## Usage

`mfp` opens a full-screen terminal UI (TUI) by default. Running `mfp` with no
subcommand is the same as `mfp play` and accepts the same options.

```bash
mfp                 # start the TUI (same as `mfp play`)
mfp play -e 75      # start at a specific episode
mfp play -s         # shuffle
mfp play -f         # favorites only
mfp play --compact  # start with the episode list hidden (player only)
mfp play --plain    # plain text UI (typed commands, see below)
mfp list            # list all episodes
mfp fav -l          # manage favorites (-a / -r to add / remove)
mfp download -e 75  # manage offline downloads
```

`-e`, `-s` and `-f` work with both UIs. `--compact` and `--plain` cannot be
combined. Run `mfp --help` or `mfp play --help` for details.

## TUI keys

| Key | Action | Key | Action |
|-----|--------|-----|--------|
| `n` | Next episode | `b` | Previous episode |
| `p` | Pause/resume | `m` | Mute |
| `+` | Volume up | `-` | Volume down |
| `s` | Toggle shuffle | `f` | Toggle favorite |
| `d` | Download episode | `i` | Episode info |
| `h` | Hide/show the episode list | `q`, `Ctrl+C` | Quit |
| `v` | Next visualizer style | `V` | Visualizer on/off |
| `Up`/`Down`, `j`/`k` | Move selection | `PageUp`/`PageDown` | Move by a page |
| `Home`/`g` | First episode | `End`/`G` | Last episode |
| `Enter` | Play the selected episode | `/` | Search episodes |

While searching (`/`), letters are typed into the query and the list is
filtered as you type. `Enter` keeps the filter, `Esc` clears it, and the
arrow, `PageUp`/`PageDown`, `Home` and `End` keys still move the selection.
`Ctrl+C` always quits.

### Layouts

- **Full**: episode list on the left, player, visualizer and status on the right.
- **Compact** (list hidden): player and visualizer at full width.

By default the layout is chosen from the terminal size: full at 80x16 or
larger, compact below that. `h` hides or shows the episode list (showing it needs at
least 64x12). The list keys (selection, `Enter`, `/`) only act while the list is shown. `--compact` starts with the list hidden.

## Configuration

Optional `~/.config/mfp/config.toml`, read once at start (mfp never writes it;
`v`/`V` changes last for the session only):

```toml
[theme]
active = "nord"      # built-in preset or themes/<name>.toml

[visualizer]
style = "wave"       # bars, mirror, wave, dots, area, vu
enabled = true
```

## Plain mode

`mfp play --plain` keeps the original text interface: type a command and press
`Enter`.

| Command | Action |
|---------|--------|
| `n` / `next` | Next episode |
| `b` / `back` / `prev` / `previous` | Previous episode |
| `p` / `pause` / `play` | Pause/resume |
| `+` / `up`, `-` / `down` | Volume up / down |
| `m` / `mute` | Mute |
| `s` / `shuffle` | Toggle shuffle |
| `f` / `fav` / `favorite` | Toggle favorite |
| `i` / `info` | Episode info |
| `d` / `download` | Download episode |
| `q` / `quit` / `exit` | Quit |

### Automatic fallback

If stdin or stdout is not a terminal (pipes, redirects) or `TERM=dumb`,
`mfp play` uses the plain UI instead of the TUI. Bare `mfp` in that situation
only prints a command overview and exits, so audio never starts unattended.

The plain UI adapts to what is redirected:

- **stdin not a terminal** (e.g. `printf 'i\nq\n' | mfp play`): commands are read
  as lines, one per line, and no live progress line is drawn. At EOF (a closed
  pipe or `< /dev/null`) playback **keeps going**: closing stdin is not a quit
  request. MPRIS controls keep working, and you stop it with MPRIS Quit or
  Ctrl+C (send `q` before EOF to quit from the input).
- **stdout not a terminal** (e.g. `mfp play --plain | cat`): the live progress
  line is omitted; command responses are still printed.

## Files

- Favorites: `~/.config/mfp/favorites.json`
- Downloads: `~/.config/mfp/downloads/`
- Diagnostics (e.g. MPRIS unavailable): `~/.config/mfp/mfp.log`

Paths follow `$XDG_CONFIG_HOME` when it is set.

## License

GPL-3.0
