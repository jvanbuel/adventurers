# Colossal Cave

The world of the game and the rules it plays by: what the player explores, carries, meets and scores.

## Language

### The world

**Location**:
A place the player can be, with a short and a long description. Limbo ("nowhere") is a location too: it holds everything out of play.
_Avoid_: room, node, square

**Travel rule**:
A way out of a location: given a motion and an optional condition, it moves the player somewhere, prints a message, or triggers a special movement.
_Avoid_: exit, edge, transition

**Forced location**:
A location the player passes through without being asked for a command; its first travel rule fires on arrival.
_Avoid_: corridor, transit room

**Deep**:
The part of the cave from the Hall of Mists inwards, where the dwarves roam and the closing clock runs.
_Avoid_: underground, inner cave

### Things

**Object**:
Anything that can be seen, named or used, including scenery and creatures. Every object has a state number whose meaning is particular to it (lit, broken, caged...).
_Avoid_: item, entity, thing

**Treasure**:
An object worth points once brought to the building intact.
_Avoid_: valuable, loot

**Two-placed object**:
An object visible from two locations, such as the grate or the troll bridge. Its second location is its far end.
_Avoid_: shared object, portal

**Immovable**:
Of an object: it can never be carried.
_Avoid_: fixed (which also names the far end of a two-placed object)

**Found**:
Of a treasure: the player has seen it. Treasures not yet found are what keep the cave open.
_Avoid_: discovered, seen

**Stashed**:
Of an object: set out in the repository by the closing of the cave, and not described until the player has picked it up and put it down again.
_Avoid_: hidden, frozen

### People

**Dwarf**:
One of five hostile wanderers in the deep cave who block the way and throw knives.

**Pirate**:
The sixth wanderer, who steals treasure and hides it with his chest in the maze.
_Avoid_: thief

### Play

**Command**:
One or two words typed by the player: a motion, a verb, an object, or a verb with an object, in either order.
_Avoid_: input, instruction

**Motion**:
A word that moves the player, such as a direction, a place name or a magic word like XYZZY.
_Avoid_: direction (most motions aren't compass points)

**Verb**:
A word for an action, such as TAKE, LIGHT or SAY.
_Avoid_: action word, command

**Magic word**:
The reservoir word, different in every game, which the player learns from the bird.
_Avoid_: password

**Hint**:
Help offered when the player has lingered where a hint applies; taking it costs points.

**Lamp life**:
The number of turns of light left in the lamp.
_Avoid_: battery, limit

**Closing**:
The period after every treasure is found, when the player is warned and can no longer leave the cave.

**Closed**:
The end game: the player is moved into the repository, a storeroom stocked with copies of the cave's contents.
_Avoid_: endgame room, finale

**Reincarnation**:
Being brought back after dying, which is offered a limited number of times.
_Avoid_: respawn, revive

**Oldstyle**:
Playing as the original program did: no prompt, words uppercased, and some one-letter abbreviations refused.
