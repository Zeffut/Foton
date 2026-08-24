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

What the corrected numbers say. The first column is where this measurement
started; the second is where `dev/parity-gaps.txt` stands now.

| missing classes | at first measurement | now |
|---|---|---|
| blocks | 63 | 17 |
| items | 38 | 4 |
| entities | 142 | 52 |

**Entities are all that is left of any size**, and nearly all of it is mobs --
though the tameable pets, the passive animals and the golems have taken a third
of that column since the last count. The three blocks that make mobs -- the
monster spawner, the trial spawner and the vault -- came off the block column
together with the spawner minecart and the ominous item spawner.
The item column is down to four, three of which are blocked on a map system
that does not exist and one of which -- vanilla's plain `Item` -- needs
nothing.

Two warnings about reading this table at all. It counts classes with *no*
behavior, so it says nothing about how complete the ones that exist are: the
sculk sensor is off the list and still never fires. And it over-counts, because
some vanilla classes are only collision shapes that Steel already takes from
extracted data -- `SoulSandBlock` is one. A line disappearing is good news; it
is not the same as the feature working.

The largest single instance of that pattern turned up late and is worth its
own paragraph, because it was the most player-visible bug in the project.
`LivingEntity::server_ai_step` does nothing unless a mob overrides it to call
`Mob::mob_server_ai_step`, and that call is the only path to the goal selector,
the path navigation and the move control. Every passive and water mob overrode
it. **None of the fifteen hostiles did, nor the iron or the snow golem.** A
zombie registered a melee attack, a stroll, two look goals and a full target
selector, and ticked none of it; so did every other hostile in the game. The
mob-local unit tests all called `mob_server_ai_step` directly and stepped
straight over the missing override, so nothing was red. There are now
seventeen tests that come in through `server_ai_step` instead, which is the
door the tick actually uses.

A pattern worth naming, because it has now come up more than twenty times: the
expensive half of a feature was often already written and simply unreachable.
Rails worked and nothing ran on them. `handle_move_vehicle` was complete and no
player could board a boat. The item frame had synced data, persistence and a
comparator output, and no item to hang one. 319 stonecutting recipes sat behind
a "skip other recipe types for now". Every mob effect applied server-side and
no client was ever told. The entire leash -- state, spring maths, packet -- was
there and no item could tie it. `is_sensitive_to_water` was declared by the
enderman and the strider and read by nothing, so neither ever took the damage.
Before adding anything, check whether the thing already exists and is merely
orphaned.

The loot system is the sharpest version of this so far, and it is worth reading
as a warning about this document too. It was written down here as "Steel has no
loot system at all". Steel had a complete one: a 2,443-line interpreter and a
build step compiling all 1355 vanilla tables into typed statics. What it did
not have was correct *results*, and nothing on the outside distinguished the
two. `match_tool` answered true when there was no tool, so 138 conditions were
wrong. Looting read the enchantment off the tool instead of the attacker, which
is not where it lives, so the enchantment did nothing at all. An entity
predicate with an unmodelled key silently matched everything, which is why
every zombie in Steel dropped a red mushroom -- vanilla gates that on riding a
zombie horse. **A system that exists is not the same as a system that works,
and this file has been wrong about which was which.**

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
      was no way to open one.

      Three more now run on the same machinery. The TNT cart lights its fuse on
      an activator rail and spares the track when it goes off, which is the
      difference between a cart cannon that works twice and one that does not.
      The furnace cart drives itself on coal, pushing away from whoever fed it.
      The hopper cart sucks up what it rolls under, and is the one cart where a
      powered activator rail switches the behavior *off*.

      Still open: spawner and command block carts, both blocked on the blocks
      they carry. `pushAndPickupEntities` is missing for all six -- a cart
      passes through everything, because Steel has no entity push. A generated
      chest cart still opens empty, but not for the reason written here before:
      see the loot entry below.

- [~] **Loot**: block drops and mob drops both roll real vanilla tables, with
      Silk Touch, Fortune, Looting, explosion decay and the killer's luck all
      doing what they should. That is a change of results, not of machinery --
      the interpreter and all 1355 tables were already here and quietly wrong.

      Container loot is unpacked too, so a generated chest finally has
      something in it. The tables were arriving all along -- worldgen writes
      the `LootTable` tag from the fortress, the stronghold, the dungeon, the
      shipwreck and the rest -- and the receiving end simply threw it away on
      load. Unpacking hangs off the lock every container access already takes,
      not off opening the lid, which is what vanilla does and what lets a
      hopper or a comparator roll a chest nobody has touched.

      Still open, and named precisely because "loot works now" would be too
      generous: the seeded draw is Rust's `StdRng`, not `java.util.Random`, so
      contents are stable per seed but not identical to a real server -- that
      belongs to the interpreter, not to any one caller. `copy_components`
      (71 uses) needs the block entity in the block loot path, which is why a
      dozen hand-written `get_drops` overrides still exist for shulker boxes,
      decorated pots, banners and skulls. `enchant_randomly` and
      `enchant_with_levels` (86 uses, all chests) need an enchantment selector
      that only an enchanting table would bring. `location_check` is still
      silently true, which double-drops seeds from tall grass.
- [x] **Storage that travels**: shulker box (17), ender chest and trapped
      chest. The trapped chest needed the container opener count first --
      without it its signal is always zero and it is a chest that costs a
      tripwire hook. That counter now lives on `BlockEntityBase` and drives
      three things at once: the trapped chest's signal, the barrel's `open`
      state, and the chest lid. Verified by `dev/openers-test.sh`.
- [~] **Workstations**: jukebox and stonecutter done. The jukebox takes a
      disc, changes shape, powers redstone while the music runs and answers a
      comparator with the record's own number. The stonecutter cuts: 319
      stonecutting recipes now reach the registry, which the build script had
      been skipping outright, and the menu lets a player pick one of the many
      cuts an input offers. The grindstone strips enchantments, keeps the
      curses, welds two damaged tools into a better one, and pays the
      experience back. The smithing table upgrades to netherite and carries
      the tool's enchantments across. A lectern takes a book, turns its
      pages, pulses redstone from the block below on every turn, and answers a
      comparator with how far through the book the reader is. The bell rings
      -- but only when struck from a side it can actually swing towards, which
      is what the hit test is for, and once per redstone rising edge rather
      than continuously. The crafter crafts: nine slots that can each be
      switched off, a comparator reading that counts a switched-off slot as
      full, one craft per redstone rising edge, and the result pushed into
      whatever container it faces or thrown if there is none. The beacon
      counts its pyramid, checks the sky above it is clear, and hands its two
      effects to everyone in range every four seconds -- which needed the
      `SetBeacon` packet, which Steel had a number for and nothing behind.
      The loom stamps banners: a banner, a dye and an optional pattern item,
      with the pattern picked from whatever that item offers -- everything it
      needs was already in the registry, so the note that called it blocked
      was simply wrong. Its output goes somewhere now too: a placed banner
      keeps its layers and gives them back when broken, which it did not
      before -- there was no banner block entity at all. Still open:
      cartography table. Armor trims are the half of smithing
      that is not here: they need trim pattern and material registries and a
      `TRIM` component Steel does not have, so those eighteen recipes are
      still skipped -- deliberately now, rather than by omission. The
      cartography table is blocked outright: Steel has no maps at all.
- [~] **Blocks that answer the world**: the lightning rod takes a strike and
      powers redstone for eight ticks, and the bolt that hits it burns what it
      lands on and scrubs oxidized copper clean. The sculk sensors, the
      shrieker and the sculk block have their state, their redstone and their
      comparator readings.

      Two of those are wired to nothing, deliberately. Steel has game events
      and a listener layer but no vibration system on top, so a sculk sensor
      never fires on its own -- `activate` is written and tested because it is
      exactly the seam a vibration system attaches to, and the vanilla
      `listener` tag is round-tripped rather than dropped. The shrieker never
      summons, because there is no warden.

      The sculk catalyst now works. `SculkSpreader` and its charge cursors were
      lifted out of `worldgen/feature/features/sculk_patch.rs` into
      `behavior/blocks/sculk/spreader.rs` and rewritten against `LevelAccessor`,
      so world generation and a live catalyst walk the same algorithm; the
      catalyst's block entity is the first in Steel to publish a game-event
      listener, hears `ENTITY_DIE` from eight blocks, takes the experience the
      death was about to drop and spends it as sculk. `dev/catalyst-test.sh`
      kills mobs next to one and watches the floor turn.

      The creaking heart carries its own state machine -- `uprooted` without
      pale oak logs on both ends of its axis, `dormant` by day, `awake` for the
      stretch of night the `creaking_active` environment attribute names -- plus
      its axis, its comparator hook and the twenty-odd experience a naturally
      generated one drops. What it cannot do is hold a creaking, because that
      mob does not exist; a vanilla world's `creaking` UUID is carried through a
      save untouched rather than dropped.

      A lightning strike still does not convert what it hits -- no pig becomes
      a zoglin, no villager a witch -- but the reason changed: the
      `thunder_hit` seam on `Entity` now exists, added alongside the golems,
      and what is missing is the mobs on the far side of it.

- [x] **Spawn eggs** (88 entries). One class, and the reason `dev/join.py`
      can now right-click a block at all.
- [~] **Tools that act**: snowball, egg and bottle o' enchanting fly and
      break, one egg in eight hatches a chick, and a bottle breaks into
      experience. Shears cap a growing vine and pay durability where the
      generic tool rule does not -- a zero-hardness plant costs a point, fire
      costs none, and finding that out is what turned the rule into a real
      `ItemBehavior::mine_block` seam instead of a special case buried in the
      block-breaking code. The crossbow charges, respects Quick Charge and
      Multishot, and will fire a firework rocket. The trident throws, sticks,
      comes home on Loyalty and launches its owner on Riptide. The wind charge
      bursts without hurting what it shoves, which took teaching
      `ExplosionSpec` to say `damages_entities: false` -- until then Steel had
      no way to express a blast that only pushes. Still open: fishing rod,
      which needs a bobber entity and a loot system.

      Two of these are honestly partial. Riptide launches the player but
      without the spin attack: Steel has no auto-spin-attack state at all.
      Channeling is left out rather than approximated, because its lightning
      needs a summon path the enchantment layer does not have.
- [~] **Decoration a player places**: flower pot (39) and item frame -- a
      plant goes in and comes back out, and a frame hangs, holds an item, turns
      it, and reads out to a comparator. Banners (16) keep their pattern layers
      from the loom through being placed and broken. Skulls and heads (9) keep
      whose head it is. The decorated pot keeps its four sherds and shatters
      back into them when broken without silk touch. A painting picks the
      largest variant its wall will take, and unlike the item frame it checks
      it can survive where it is being put. The armor stand is the first
      `LivingEntity` here that is not a mob: it swaps gear on a right-click and
      needs two hits inside five ticks to break.

      A lead ties a mob to a fence. Almost all of that was already here --
      the leash state, the spring maths, the packet the tracker sends -- and
      only the item was missing, so a player could walk a pig around and never
      hitch it.

      The bundle works, all of it: the weight rule, partial insertion, the
      selected item and the cycling. Books can be written, signed and read,
      which needed the `SEditBook` and `OpenBook` packets -- Steel had numbers
      for both and nothing behind them. The debug stick cycles block states for
      an operator, fish buckets carry cod and salmon, and the powder snow
      bucket places its block.

      Still open: glow item frame, whose entity is a separate vanilla class.
      Three shared gaps are worth naming rather than repeating per entity: a
      frame still does not drop what is in it; nothing block-attached falls off
      when its wall goes, because there is no block-attached tick pass; and an
      armor stand has no pose, because `Rotations` has no NBT codec and a
      half-written one would lose a map-maker's work in silence.

### 2. The living world

- [~] **The golems a player builds.** A carved pumpkin on the right stack of
      blocks now makes a golem, which needed `BlockPattern`, `BlockInWorld` and
      `BlockPatternBuilder` -- ported whole into `world/block_pattern/` rather
      than into the pumpkin, because the wither summon and the end crystal
      ritual want the same machinery. The snow golem leaves a trail, throws
      snowballs and melts; the iron golem cracks as it is hurt, is repaired
      with an ingot, remembers being player-built and keeps a grudge across a
      save; the copper golem weathers, shears, waxes and freezes into a statue.

      Two honest limits. An iron golem's *village* half is absent -- no
      `MoveBackToVillage`, no `DefendVillage`, no natural village spawn --
      because those need a POI distance tracker Steel has not got, and
      villagers. And the copper golem never moves an item: vanilla drives it
      entirely from a `Brain`, and Steel has no brain layer, only goals.

- [ ] **Villager, trading, iron golem in a village.** One mob unlocks a whole
      economy, and the zombie villager behind it. It is also what would make
      the iron golem's village goals and its poppy worth writing: with no
      villagers, an iron golem currently offers flowers only to copper golems.
- [ ] **Mounts**: horse, donkey, mule, skeleton horse, llama, camel.
- [ ] **The rest of the passive roster**: cat, ocelot, fox, rabbit, panda,
      goat, turtle, dolphin, parrot, bee, axolotl, frog, sniffer, armadillo.
- [ ] **The rest of the hostile roster**: blaze, ghast, phantom, guardian,
      shulker, piglin and brute, hoglin, endermite, vex, breeze, creaking.
- [ ] **Raids**: evoker, vindicator, pillager, ravager, illusioner.

Ghast and phantom need flying navigation, which does not exist yet; that is one
piece of work that unlocks several mobs at once, and should come before the
mobs that need it.

- [~] **A Brain, and one mob on it.** Vanilla drives its newer mobs from a
      `Brain` -- memories, sensors, activities, a behaviour schedule -- and not
      from goals. Steel now has one: 33 memory kinds, 6 sensors, 18 behaviors,
      and the copper golem running `CopperGolemAi` for real, carrying stacks
      between containers.

      Vanilla's `BehaviorBuilder` DSL was deliberately not ported. It emulates
      higher-kinded types on DataFixerUpper to say "memory A present, memory B
      absent, here are typed accessors" in a language without variadic
      generics; Rust says that in two lines with `let ... else`.

      What a brain mob still needs, each blocking a named list: a POI ticket
      and distance tracker (villagers, the iron golem's village half), an
      `EnvironmentAttribute<Activity>` schedule (the villager day), a
      multi-slot `InventoryCarrier` (villagers, piglins), long-jump and ram
      behaviors (the goat), and vibration listeners (the warden).

- [~] **Spawners.** The monster spawner, the trial spawner with its whole
      state machine, the vault with its per-player reward ledger, the spawner
      minecart and the ominous item spawner. The 28 vanilla trial-spawner
      configs are compiled from the extracted datapack rather than transcribed.

      Two things this found: Steel's strict NBT getters would have silently
      reset every saved spawner, because vanilla writes those delays as shorts
      and reads them leniently; and `simdnbt`'s `insert` appends instead of
      replacing, which was quietly giving a spawn entry two `id` keys.

- [x] **The horse family**: horse, donkey, mule, skeleton horse with its
      lightning trap, zombie horse, llama, trader llama and the llama spit.
      Taming by temper, the chest, coat and markings, the caravan, breeding
      inheritance. The inventory itself is complete -- contents, resizing, slot
      rules, NBT and drops -- but there is no screen to open it with, because
      the saddle and armor slots are entity equipment and Steel's menu slots
      are backed by containers.

### 2b. Blocks the world runs on its own

- [~] Nylium dies back without light and grows nether vegetation from bone
      meal; netherrack takes the spread. Frosted ice melts through its four
      stages. The dried ghast hydrates in water and dries on land. The shelf
      holds three items and chains sideways up to three wide, powered or not.
      Copper chests weather with the rest of the copper. The light block cycles
      its level for an operator only.

      Left on the ledger deliberately: `BonemealableFeaturePlacerBlock` (moss),
      because placing its patch needs the worldgen placement dispatcher, and a
      `perform_bonemeal` that placed nothing would still eat the bone meal --
      worse than the no-op there now.

### 3. The end of the game

- [ ] Ender dragon, wither, elder guardian, end crystal, eye of ender.

### Blocked, and honestly so

- **Statistics and advancements.** Both need `stat_type` and `custom_stat`
  registries that come from SteelExtractor, an external tool not in this
  repository. `AGENTS.md` forbids hand-writing extracted data, so these cannot
  be done here. Every `TODO: award stat ...` in the tree is waiting on this.

## Running all of it

`bash dev/all-tests.sh` runs every in-world test once, in sequence, and prints
a tally. They are run one at a time on purpose: each starts its own server on
its own port, but two at once still tread on each other's run directories and
on the machine, and the failures that produces look exactly like real ones.

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
- `dev/throw-test.sh` -- a snowball is thrown, flies and breaks, forty eggs
  hatch at least one chicken, and a bottle o' enchanting breaks into experience.
- `dev/hopper-minecart-test.sh` -- a cart parked under a loaded chest empties
  it, read off the chest's comparator; the control cart on a powered activator
  rail leaves its chest alone, which is the rule that is backwards on this cart.
- `dev/furnace-minecart-test.sh` -- a fed cart drives itself down a track and
  comes to rest on a detector rail, which is the only part of it a command can
  read.
- `dev/tnt-minecart-test.sh` -- a cart on a live activator rail lights its
  fuse and goes off, taking a witness block with it and leaving the track
  standing, which is the half that must not happen.
- `dev/decoration-test.sh` -- an armor stand takes a helmet on the head and a
  painting hangs on a wall. Both are read off packets: a stand's gear and a
  painting's variant are entity state with no command behind them.
- `dev/beehive-test.sh` -- shears and a glass bottle each empty a full hive,
  and neither touches one that is not full. A hive's honey level is a block
  state, so this is one of the few interactions readable straight back.
- `dev/beacon-test.sh` -- a beacon with no pyramid refuses the effect and
  hands the payment back; one with a pyramid takes it and the effect reaches
  the player. Read off the effect packet, since no command can see any of it.
- `dev/workstation-test.sh` -- right-clicking each workstation opens its
  menu, and the crafter runs a recipe end to end: a log shift-clicked into
  the grid, a redstone pulse, and the grid empty again, all read off a
  comparator. The loom's own stamping is covered by unit tests rather than
  here -- no command reads a banner's pattern layers back, and the result
  never becomes a block.
  What they *do* is tested in Rust, where the computation lives: a recipe
  button and a slot click need container packets the scripted client cannot
  send.
- `dev/openers-test.sh` -- an open trapped chest powers redstone and a
  closed one stops, a plain chest powers nothing, a chest under a solid
  block refuses to open, and a barrel looks open while somebody is in it.

Every entry above gets one before it is called done.
