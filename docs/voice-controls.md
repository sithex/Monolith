# Voice controls on the Hermes page

The Hermes page (the island's home, the terminal) can show two small controls at the right
end of its title row:

- **Voice picker**: a dropdown of your saved text-to-speech voices. Picking one switches the
  voice Hermes speaks with.
- **Mute button**: a speaker icon that silences Hermes' spoken replies (and cuts off anything
  already playing). It turns red while muted. Click it again to unmute.

Both are optional. Each control only appears when its helper is installed, so an install
without them simply shows a plain Hermes page. The island never edits your Hermes
configuration itself: it calls the two helpers below and shows what they report.

## What the island looks for

| Control | Helper | Where the island looks |
|---|---|---|
| Voice picker | `hermes-fish` (executable) | your `PATH`, then `~/.local/bin/hermes-fish` |
| Mute button | `tts_mute.py` (Python 3 script) | `~/Hermes/Voices/tts_mute.py` |

The island re-reads both every 10 seconds while the Hermes page is open, so changes made
elsewhere (for example running `hermes-fish <name>` in the terminal, or a panel shortcut that
toggles mute) show up on their own.

### `hermes-fish` contract

The name comes from the Fish Audio provider it was written for, but the island does not care
which TTS service is behind it. Any executable with this behaviour works:

| Command | Must do | Output the island reads |
|---|---|---|
| `hermes-fish` | list saved voices | One optional header line `Fish Audio voices:`, blank lines allowed, then one line per voice: two spaces, the name, and `  [active]` (two spaces) after the active one |
| `hermes-fish current` | print the active voice | the voice name on the first line |
| `hermes-fish <name>` | switch to that voice | anything; exit code 0 on success |

Example list output:

```
Fish Audio voices:

  narrator
  robot  [active]
  wizard
```

Voice names are 1 to 64 characters from `A-Z a-z 0-9 . _ -`. The words `current`, `add`,
`remove`, `id` and `list` are reserved and never offered as voices. Each call must finish
within 4 seconds.

### `tts_mute.py` contract

Run as `/usr/bin/python3 ~/Hermes/Voices/tts_mute.py <op>`:

| Op | Must do | Output |
|---|---|---|
| `status` | report the current state | exactly `muted` or `unmuted` |
| `toggle` | flip the state; when muting, stop any speech that is playing | anything |

Each call must finish within 3 seconds. How muting works is up to you; it only has to make
your Hermes stop speaking. The common approach is a flag file that your TTS playback checks
before playing, plus stopping the audio player process that is currently speaking.

## Set it up with your own Hermes

The quickest way is to let Hermes build the helpers for your setup. Open Hermes (the island
terminal works) and paste this prompt:

```text
I use the Monolith island for Hermes. Its Hermes page shows a voice picker and a mute button
when two small helpers exist. Please set them up for MY current text-to-speech setup.

1. Look at my Hermes TTS configuration (config.yaml `tts:` section and any custom provider
   scripts) and tell me which provider and voice setting I use before changing anything.

2. Create an executable `hermes-fish` in ~/.local/bin (unless one is already on my PATH):
   - `hermes-fish` with no arguments prints the header line `Fish Audio voices:`, a blank
     line, then each saved voice as two spaces + name, adding two spaces + `[active]` after
     the voice currently in use.
   - `hermes-fish current` prints only the active voice name.
   - `hermes-fish <name>` switches my Hermes TTS to that saved voice and exits 0.
   - `hermes-fish add <name> <voice id>` and `hermes-fish remove <name>` manage the saved
     list. Names may only use letters, digits, `.`, `_` and `-`.
   - Store the saved voices somewhere simple (e.g. one file per voice holding its provider
     voice/reference id) and make switching change the voice my TTS provider really uses.
   - Every command must finish in under 4 seconds.

3. Create ~/Hermes/Voices/tts_mute.py (Python 3, standard library only):
   - `tts_mute.py status` prints exactly `muted` or `unmuted`.
   - `tts_mute.py toggle` flips a mute flag file. When muting, it also stops any Hermes
     speech that is currently playing (only Hermes' own audio player processes, never other
     apps), and audio generated before the toggle must not play afterwards.
   - Make my Hermes TTS playback respect the flag, in the least invasive way for my setup.
   - Every command must finish in under 3 seconds.

4. Back up any file you change first, keep my current voice active, and show me how you
   tested each command (`hermes-fish`, `hermes-fish current`, switching back and forth,
   `tts_mute.py status`, and toggling mute twice so I end up unmuted).
```

When Hermes is done, open the island's Hermes page. The picker and the mute button appear
within about 10 seconds (or right away after you switch pages and come back).

## Set it up by hand

1. Put an executable `hermes-fish` that follows the contract above on your `PATH` or in
   `~/.local/bin`. Check it:

   ```sh
   hermes-fish            # list with one [active] line
   hermes-fish current    # prints the active name
   ```

2. Put `tts_mute.py` at `~/Hermes/Voices/tts_mute.py`. Check it:

   ```sh
   /usr/bin/python3 ~/Hermes/Voices/tts_mute.py status   # muted | unmuted
   ```

3. Open the Hermes page in the island.

## Troubleshooting

- **No controls at all**: neither helper was found or both failed. Run the checks above in a
  terminal; the island needs `hermes-fish` to print at least one voice line and
  `tts_mute.py status` to print exactly `muted` or `unmuted`.
- **Picker shows but switching fails**: `hermes-fish <name>` must exit 0, and afterwards
  `hermes-fish current` must print that same name. If two saved names share the same voice
  id, `current` reports the first one; give each saved name its own id.
- **Island started from autostart can't find `hermes-fish`**: the login session's `PATH` may
  not include your shell's extra directories. Put the helper (or a symlink) in
  `~/.local/bin`, which the island always checks.
- **Mute button does nothing audible**: the island only runs `tts_mute.py toggle`; your
  Hermes TTS playback has to check the flag. Ask Hermes to verify that part with the prompt
  above.
