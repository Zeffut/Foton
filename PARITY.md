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
- [~] **Boats and rafts** (20 entity entries, 20 item entries). `Boat` and
      `Raft` float, carry riders, are steered by the rider's client, and can be
      placed from the item -- a player can cross water. Still open: `ChestBoat`
      and `ChestRaft` (ten entity entries, and a container; their items exist
      and refuse cleanly), the `SPaddleBoat` packet so rowing animates, and the
      second seat.
- [ ] **Minecarts** (6 entries). Rails already work; the carts do not exist.
- [~] **Storage that travels**: shulker box (17) done -- contents survive being
      broken and come back when placed. Still open: ender chest, trapped chest.
- [ ] **Workstations**: grindstone, stonecutter, smithing table, cartography
      table, loom, lectern, jukebox, bell, beacon, crafter.
- [x] **Spawn eggs** (88 entries). One class, and the reason `dev/join.py`
      can now right-click a block at all.
- [ ] **Tools that act**: shears, fishing rod, flint and steel's siblings,
      snowball, egg, experience bottle, ender pearl's siblings.
- [~] **Decoration a player places**: flower pot (39) done -- a plant goes in
      and comes back out. Still open: item frame, painting, armor stand,
      banner (16), skull (9).

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
- `dev/spawnegg-test.sh` -- a spawn egg used by hand makes the mob.
- `dev/boat-test.sh` -- a boat put on water is still on it afterwards.

Every entry above gets one before it is called done.
