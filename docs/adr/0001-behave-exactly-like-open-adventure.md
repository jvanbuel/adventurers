# Behave exactly like Open Adventure

The port must produce byte-identical output to the C Open Adventure for every input, and read and write the same save files. That is what lets the upstream regression logs and a differential fuzzer against the C build serve as the test suite. So the code deliberately keeps the original's quirks, even where cleaner code would behave differently:

- some command rewrites compare vocabulary numbers across word kinds;
- "no dwarves" travel rules still roll a die;
- scanning travel rules for the dwarves leaves the last destination in `newloc`, which can end up in a save file;
- `strtol` semantics apply to `seed` and `waste` arguments;
- the location object lists keep the C intrusive-list layout.

Each of these is commented where it happens.

We depart from the original only where it has undefined behaviour or hangs, and the departure is the obvious sane outcome:

- **Intransitive "take" with only a fixed object present.** When the only thing in a room is the far end of a two-placed object, the C code indexes past its object table. The port takes the object itself, so it gets the normal refusal.
- **Taking the cage after killing a caged bird.** The C code hangs; the port takes just the cage.
- **Out-of-range save fields.** A save with negative locations or out-of-range `oldobj`, `knfloc` or `bonus` values is rejected as tampered rather than played with garbage.
- **Truncated save files.** These read as zero-filled, matching a first restore in C.

## Considered Options

- **Clean up the quirks.** This was rejected: every cleanup removes the oracle that proves the port correct, and the quirks only show up for nonsensical inputs.
