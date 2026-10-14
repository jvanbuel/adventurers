# adventurers

A Rust port of [Open Adventure](https://gitlab.com/esr/open-adventure),
Eric S. Raymond's forward port of Will Crowther and Don Woods'
*Colossal Cave Adventure* 2.5 (the 430-point version). This port
tracks upstream release 1.22.

It behaves exactly like the C program (see
[ADR 0001](docs/adr/0001-behave-exactly-like-open-adventure.md)):

* All 108 regression logs from the upstream test suite produce
  byte-identical output (`cargo test`).
* The RNG, word parsing, and every message match the original.
* Save files use the same binary layout as the C build on
  little-endian machines, so a save from one resumes in the other.

The code itself is not a transliteration. It is organized as idiomatic
Rust around a small, testable interface (see [Design](#design)).

## Building and playing

```sh
cargo run --release            # play
cargo run --release -- -o      # "oldstyle": no prompt, uppercase parsing
cargo run --release -- -r game.adv   # resume a saved game
```

Options are the same as the C `advent` (`advent --help` lists them):

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

## Using the library

The game runs in-process too.  An `Engine` plays it through a `Console`;
`ScriptedConsole` feeds it a script and records the transcript:

```rust
use adventurers::{Config, Engine, GameOver, ScriptedConsole, Start};

let console = ScriptedConsole::new(["no", "in", "take lamp", "quit", "yes"]);
let mut engine = Engine::new(console, Config::default(), 42);
let over = engine.play(Start::New);

assert!(matches!(over, GameOver::Finished { .. }));
assert!(engine.console().transcript().contains("> take lamp\n\nOK\n"));
```

## Design

The domain vocabulary is in [GLOSSARY.md](GLOSSARY.md).

**One deep module with a small interface.** `Engine` is the whole game
behind `new`, `start`, `run` and `play`.  Everything else (movement,
dwarves, hints, closing, the verbs, scoring) is private to it, so it
can change without touching callers or tests.

**A real seam for I/O.** The engine never touches stdin or stdout, and
never exits the process.  It talks through the `Console` trait, which
has two adapters: the terminal (`src/bin/advent/terminal.rs`, which
handles prompts, script files and the log) and `ScriptedConsole` for
tests.  Ending the game is a returned `GameOver` value; every step that
can end it returns `Result<_, GameOver>` and uses `?`.

**Types instead of sentinels.**

* Locations, objects, messages, motions and verbs are distinct `Copy`
  newtypes (`Loc`, `Obj`, `Msg`, `Motion`, `Verb`), generated from the
  YAML as typed constants.
* Where an object is, is `Place::{Carried, At(loc)}`, and its far end is
  `Fixed::{Free, Immovable, At(loc)}`, rather than `-1`/`0` codes.
* "No object given" is `Option<Obj>`.
* Travel rules are per-location slices of `TravelOp { cond, dest }`
  with `TravelCond` and `Dest` enums.
* Save decoding parses into a valid game or a typed `RestoreError`.

**Placement is one module.** `objects.rs` owns where every object is
and the order each room's contents are listed in.  It keeps the
original's linked lists, which are part of the save format, entirely
private.

| Path                        | Contents                                           |
|-----------------------------|----------------------------------------------------|
| `adventure.yaml`            | The dungeon database (unchanged from upstream)     |
| `build.rs`                  | Compiles the YAML into typed Rust tables           |
| `src/dungeon.rs`            | Typed ids and the static tables                    |
| `src/engine/`               | The game: `Engine`, `Config`, `GameOver`           |
| `src/engine/turn.rs`        | Main loop: move, describe, read and run commands   |
| `src/engine/movement.rs`    | Travel rules, "back", special movements            |
| `src/engine/dwarves.rs`     | Dwarves and the pirate                             |
| `src/engine/hints.rs`       | Offering hints                                     |
| `src/engine/closing.rs`     | Lamp, cave closing, the end game                   |
| `src/engine/actions/`       | What each verb does, grouped by what it acts on    |
| `src/engine/score.rs`       | Death, scoring, game over                          |
| `src/objects.rs`            | Object positions, states and room lists            |
| `src/state.rs`              | The saveable game record                           |
| `src/save.rs`               | Save-file format and validation                    |
| `src/parser.rs`             | Words and vocabulary lookup                        |
| `src/text.rs`               | Message templates                                  |
| `src/rng.rs`                | The game's random number generator                 |
| `src/console.rs`            | The `Console` seam and `ScriptedConsole`           |
| `src/bin/advent/`           | The `advent` command and its terminal console      |
| `src/bin/cheat.rs`          | Writes odd save files for testing                  |
| `tests/regress/`            | Upstream regression logs                           |

## Testing

```sh
cargo test
```

This runs three kinds of tests:

* **Unit tests** for the parser, message rendering, RNG, object
  placement and save format.
* **In-process tests** (`tests/engine.rs`) that play through the
  `Engine` interface with a `ScriptedConsole`.
* **The upstream regression suite** (`tests/regress.rs`), which runs
  every log through the `advent` binary and compares the output byte
  for byte.

During the port, the binary was also checked against the C build with
thousands of randomized command scripts.

## Deviations from the C code

* **Undefined behaviour and hangs.** Where the original reads out of
  bounds or loops forever, the port does the obvious sane thing instead.
  [ADR 0001](docs/adr/0001-behave-exactly-like-open-adventure.md) lists
  each case.  One of them is a hang in the original: put the caged bird
  down, kill it, then take the cage.
* **Input.** There is no line editing or history (the C version uses
  libedit); input is plain stdin.
* **Signals.** Flushing the log on SIGINT and autosaving on
  SIGHUP/SIGTERM are not ported.  The log is flushed after every command
  instead.
* **Command-line errors.** Bad options get clap's messages, with exit
  status 2.

## License

BSD-2-Clause, the same as Open Adventure; see `COPYING`. The original
game is by Will Crowther and Don Woods. Open Adventure is maintained by
Eric S. Raymond and contributors.
