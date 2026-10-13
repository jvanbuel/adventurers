# adventurers

A Rust port of [Open Adventure](https://gitlab.com/esr/open-adventure),
Eric S. Raymond's forward port of Will Crowther and Don Woods'
*Colossal Cave Adventure* 2.5 (the 430-point version). This port
tracks upstream release 1.22.

It is a line-by-line translation of the C code, so the two behave the
same:

* All 108 regression logs from the upstream test suite produce
  byte-identical output (`cargo test`).
* The RNG, word parsing, and every message match the original.
* Save files use the same binary layout as the C build on
  little-endian machines, so a save from one resumes in the other.

## Building and playing

```sh
cargo run --release            # play
cargo run --release -- -o      # "oldstyle": no prompt, uppercase parsing
cargo run --release -- -r game.adv   # resume a saved game
```

Options are the same as the C `advent`:

| Option        | Meaning                                              |
|---------------|------------------------------------------------------|
| `-l FILE`     | Log your commands to FILE (replayable as a script)   |
| `-o`          | Oldstyle: no prompt, emulate the original's parsing  |
| `-r FILE`     | Restore from a saved game                            |
| `-d`          | Debug: trace random-number calls                     |
| `script ...`  | Read commands from script files (`-` is stdin)       |

Cargo features match the C build flags:

* `--features nosave` disables save/resume (`-DADVENT_NOSAVE`).
* `--features autosave` replaces save/resume with `-a FILE` automatic
  saving (`-DADVENT_AUTOSAVE`).

The `cheat` binary writes deliberately odd save files for testing,
like its C counterpart. For example, `cheat -d -900 -o odd.adv`.

## Layout

| Path                | Ported from         | Contents                               |
|---------------------|---------------------|----------------------------------------|
| `adventure.yaml`    | `adventure.yaml`    | The dungeon database (unchanged)       |
| `build.rs`          | `make_dungeon.py`   | Compiles the YAML into Rust tables     |
| `src/dungeon.rs`    | `dungeon.h`         | Table types; includes the generated data |
| `src/advent.rs`     | `advent.h`          | Game state, constants, predicates      |
| `src/game.rs`       | `main.c`            | Main loop, movement, dwarves, hints    |
| `src/actions.rs`    | `actions.c`         | Verb handlers                          |
| `src/misc.rs`       | `misc.c`            | I/O, parser, object lists, RNG         |
| `src/saveresume.rs` | `saveresume.c`      | Save/restore and validation            |
| `src/score.rs`      | `score.c`           | Scoring and endgame                    |
| `src/init.rs`       | `init.c`            | New-game setup                         |
| `src/bin/cheat.rs`  | `cheat.c`           | Save-file generator                    |
| `tests/regress/`    | `tests/`            | Upstream regression logs               |

## Deviations from the C code

* **Intransitive `take` with only a fixed object present.** When the
  only thing in a room is the far end of a two-location object (the
  grate seen from below, for example), the C code indexes past the end
  of its object table, which is undefined behaviour. The port maps that
  entry back to the object itself, so you get the normal "can't carry
  that" response. Apart from that, randomized command scripts give
  identical output from both programs.
* Line editing and history from libedit are not included; input is
  plain stdin.
* Signal handling (flushing the log on SIGINT, autosaving on
  SIGHUP/SIGTERM) is not ported. The log file is flushed after every
  command instead.

## License

BSD-2-Clause, the same as Open Adventure; see `COPYING`. The original
game is by Will Crowther and Don Woods. Open Adventure is maintained by
Eric S. Raymond and contributors.
