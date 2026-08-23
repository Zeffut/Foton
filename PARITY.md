# Vanilla parity: where Steel stands, and the order of the work

The goal is a server a player cannot tell from vanilla. This is the measured
distance to that, and the order it gets closed in.

Numbers come from `python3 dev/coverage.py` and the machine-checked ledger in
`dev/parity-gaps.txt`. Nothing here is an estimate by eye.

## How the measurement was wrong, and what it says now

`dev/coverage.py` counted behavior classes. Three things made that number lie,
all found while writing this:

- **Plain blocks and items are not gaps.** 181 block entries and 236 item
  entries are backed by vanilla's plain `Block` and `Item`. They need no
  behavior; `DefaultBlockBehavior` covers them. Counting them as missing hid
  how good the real coverage is.
- **A behavior a macro declares is invisible.** The codegen finds behaviors by
  scanning for `#[block_behavior]` on a *named* struct. All three furnaces were
  declared inside `macro_rules!`, so the generator skipped them silently and
  **no furnace had a behavior at all** -- right-clicking one did nothing, and
  smelting was unreachable in a running server. Fixed, and the ledger now makes
  the whole class of mistake visible.
- **Entity coverage is per registry entry, not per class.** Nine boat woods are
  one `Boat` class but nine things a player can hold.

What the corrected numbers say:

| | entries with a behavior | of total | genuinely missing classes |
|---|---|---|---|
| blocks | 841 | 1196 | 62 |
| items | (see note) | 1537 | 38 |
| entities | 40 | 158 | 102 |

Blocks are close. Items are mostly `BlockItem`, which the item codegen handles
separately from behaviors. **Entities are where the distance is**, and inside
entities the distance is concentrated: 20 of the 118 missing entries are boats
and rafts, 6 are minecarts, 5 are the horse family.

## The order

Ordered by what a player meets first, not by what is easy.

### 1. A survival world that works end to end

Everything here is something a player does in the first hour and currently
cannot.

- [x] **Smelting.** Furnace, smoker, blast furnace had no behavior.
- [x] **Renewable wood.** Saplings never grew.
- [~] **Boats and rafts** (30 entity entries, 30 item entries). `Boat`,
      `Raft`, `ChestBoat` and `ChestRaft` float, are placed from the item, and
      are boarded by right-clicking -- until that last part landed a boat could
      be built and steered but never got in, because nothing turned a
      right-click into a rider. A chest boat's twenty-seven slots open on a
      sneak. Still open: the `SPaddleBoat` packet so rowing animates, the
      second seat, and the fact that a boat cannot be broken at all, which is
      also why a chest boat never spills its contents -- there is no way to
      destroy one yet.
- [~] **Minecarts** (6 entity entries, 6 item entries). The plain cart rolls:
      a powered rail launches it, it coasts and slows, it follows a curve
      without anything looking for one, a detector rail notices it pass, the
      item puts one on a rail and refuses anything else, and a player can ride
      it. This is vanilla's old physics, the one a world runs with
      `minecart_improvements` off. The chest minecart rolls too and opens
      instead of seating; mineshafts have always generated these and there
      was no way to open one. Still open: furnace, hopper, TNT, spawner and
      command block, and `pushAndPickupEntities` -- a cart passes through
      everything, because Steel has no entity push. A generated chest cart
      opens empty: Steel has no loot system at all, which is a gap of its own
      and older than this.
- [x] **Storage that travels**: shulker box (17), ender chest and trapped
      chest. The trapped chest needed the container opener count first --
      without it its signal is always zero and it is a chest that costs a
      tripwire hook. That counter now lives on `BlockEntityBase` and drives
      three things at once: the trapped chest's signal, the barrel's `open`
      state, and the chest lid. Verified by `dev/openers-test.sh`.
- [~] **Workstations**: jukebox done -- a disc goes in, the model changes, it
      powers redstone while the music runs and answers a comparator with the
      record's own number, and right-clicking gives the disc back. Still open:
      grindstone, stonecutter, smithing table, cartography table, loom,
      lectern, bell, beacon, crafter.
- [x] **Spawn eggs** (88 entries). One class, and the reason `dev/join.py`
      can now right-click a block at all.
- [~] **Tools that act**: snowball and egg done -- both fly, both break, and
      one egg in eight hatches a chick. Still open: shears, fishing rod,
      trident, crossbow, experience bottle, wind charge.
- [~] **Decoration a player places**: flower pot (39) and item frame done -- a
      plant goes in and comes back out, and a frame hangs, holds an item, turns
      it, and reads out to a comparator. Still open: glow item frame (the
      entity is a separate vanilla class Steel does not have), painting (same),
      armor stand, banner (16), skull (9). A frame does not drop what is in it
      when broken, and does not fall off when its wall goes.

### 2. The living world

- [ ] **Villager, trading, iron golem.** One mob unlocks a whole economy, and
      the zombie villager behind it.
- [ ] **Mounts**: horse, donkey, mule, skeleton horse, llama, camel.
- [ ] **The rest of the passive roster**: cat, ocelot, fox, rabbit, panda,
      goat, turtle, dolphin, parrot, bee, axolotl, frog, sniffer, armadillo.
- [ ] **The rest of the hostile roster**: blaze, ghast, phantom, guardian,
      shulker, piglin and brute, hoglin, endermite, vex, breeze, creaking.
- [ ] **Raids**: evoker, vindicator, pillager, ravager, illusioner.

Ghast and phantom need flying navigation, which does not exist yet; that is one
piece of work that unlocks several mobs at once, and should come before the
mobs that need it.

### 3. The end of the game

- [ ] Ender dragon, wither, elder guardian, end crystal, eye of ender.

### Blocked, and honestly so

- **Statistics and advancements.** Both need `stat_type` and `custom_stat`
  registries that come from SteelExtractor, an external tool not in this
  repository. `AGENTS.md` forbids hand-writing extracted data, so these cannot
  be done here. Every `TODO: award stat ...` in the tree is waiting on this.

## Keeping this honest

`dev/parity-gaps.txt` is generated from `classes.json` at build time and checked
by a test. A gap that closes has to be crossed off deliberately; a gap that
*opens* -- a behavior that stops being registered, the way the furnaces did --
fails the build. Run `python3 dev/update-parity-gaps.py` after reading the diff,
never before.

The other half of honesty is in-world verification. A unit test cannot see that
a furnace has no behavior. These scripts can:

- `dev/join-test.sh` -- a real client reaches the world.
- `dev/reload-test.sh` -- a world survives a clean stop and a hard kill.
- `dev/nether-test.sh` -- a client crosses dimensions and sees Nether mobs.
- `dev/sapling-test.sh` -- a planted sapling becomes a tree.
- `dev/container-test.sh` -- placed container blocks have block entities.
- `dev/flowerpot-test.sh` -- a flower goes into a pot and comes back out.
- `dev/enderchest-test.sh` -- right-clicking an ender chest opens a container.
- `dev/spawnegg-test.sh` -- a spawn egg used by hand makes the mob.
- `dev/boat-test.sh` -- a boat put on water is still on it afterwards.
- `dev/ride-test.sh` -- right-clicking a boat puts the player in it, sneaking
  does not, and a chest boat opens its chest on a sneak and boards on a click.
- `dev/minecart-test.sh` -- a cart launched by a powered rail runs a line, takes
  a corner, stops on a detector rail that notices it, is placed from the item
  onto a rail and nowhere else, and carries a player.
- `dev/jukebox-test.sh` -- a disc goes into a jukebox and comes back out, the
  block powers a lamp only while it plays, and a comparator reads the record.
- `dev/frame-test.sh` -- an item frame is hung from the item, filled, turned,
  and read through the wall behind it by a comparator.
- `dev/throw-test.sh` -- a snowball is thrown, flies and breaks, and forty eggs
  hatch at least one chicken.
- `dev/openers-test.sh` -- an open trapped chest powers redstone and a
  closed one stops, a plain chest powers nothing, a chest under a solid
  block refuses to open, and a barrel looks open while somebody is in it.

Every entry above gets one before it is called done.
