# Zelda armor compatibility

Explicit user requirement, 2026-10-03. Source audit uses the unchanged Zelda
Git object `3c324d63c502eb18242af7c771baa11ea1c2915c`, not its dirty working
checkout. Paper 1.21.11 build 132 is the plugin behavior/ABI oracle; Foton's
native mechanics use Minecraft 26.2 source and extracted data. This document
records requirements and gaps; the separately identified Paper-only oracle
below is reference evidence, not a Foton compatibility pass.

## Actual Zelda workflows

Purchased ornaments: `trim/TrimManager.java`, `TrimListener.java`,
`Ornement.java` and `TrimGUI.java` select eligible armor, preserve an existing
smithing trim, apply the selected pattern/material and record the owner's PDC
marker. Replacing a purchase replaces only the owner's previous ornament.
Removing access/selection or finding a foreign owner removes only the plugin
trim and marker. All four worn slots participate. Preview items must never
escape the cancelled GUI.

Ownership cleanup spans the next-tick armor-change/click/drag path, cursor,
offhand, storage, ender chest, containers and armor stands, plus drop/pickup,
join/quit, death/keepInventory/respawn and shutdown. Passing one equipment
getter does not prove this lifecycle. Plugin-owned trims must not leak to
another player or lose the underlying item's other metadata.

Piaf flight: `race/RacePassives.java` and `ChestplateBackup.java` persist the
original chest item before replacing it with an unbreakable, marked elytra.
The replacement copies enchantments/custom modifiers and adds the plugin's
chest armor/toughness/knockback modifiers. Landing, quit, death and restart
must restore the original item; the fake elytra must not drop. Recovery with
an occupied chest slot must not destroy or duplicate either item.

No armor-specific potion bonus or leather dye operation was found in this
pinned Zelda revision. Race potion effects and shield coloring are separate
features. Generic native armor behavior still matters to real protection and
the Piaf replacement; do not invent an armor feature to fill an audit table.

## Source-observed Foton gaps

| Surface | Gap requiring implementation or proof |
| --- | --- |
| Trim access | ITEMS_TRIMMABLE_ARMOR tag, TRIM data-component access, ItemStack.hasData and stack PDC view are missing; ordinary armor selects plain meta instead of ArmorMeta |
| Native item bridge | Armor trims and ItemMeta custom attribute modifiers do not cross the existing inventory codec; item flags also need preservation |
| Equipment lifecycle | PlayerArmorChangeEvent is absent; timing and mutation of related drop/death/armor-stand events need real scenarios |
| Dropped items | Direct UUID-backed setItemStack already reaches the native entity; full component fidelity remains unverified, and cancellation restores the original instead of the listener-modified stack |
| Chestplate backup | Current Foton-specific serializeAsBytes omits PDC, trims, custom modifiers and opaque data components |
| Combat/appearance | Native armor/equipment/damage foundations exist, but Java-created replacement equipment must actually affect protection and be visible to both clients |

These are source findings. They do not claim a measured runtime pass/failure
for each scenario, and they do not remove the existing Zelda startup blockers.

### Shared item prerequisites confirmed by deeper preflight

The current slot projection is not a full component patch: it rebuilds an
item from its prototype and selected fields. Its separate opaque NBT string
does not preserve arbitrary typed components or prototype removals. Concrete
regressions must cover a GLIDER addition and a DAMAGE removal surviving a
trim-only Java edit. Canonical component transport must precede generic
presence/get/set/remove/reset claims; removal and restoration of prototype
defaults are different operations.

Pinned Paper bytecode also resolves several ABI contracts: Tag and trim
pattern/material types are interfaces, not Foton's current final classes;
TRIM's value is ItemArmorTrim, whose armorTrim() projection Zelda calls.
LeatherArmorMeta is separate from ArmorMeta; ColorableArmorMeta combines them,
and meta capabilities are not identical to trimmable-tag membership.

The stack PDC view is owner-bound and read-only: a retained view follows later
applied meta/handle changes, but not unapplied edits to a copied meta. The
complete view/mutable-PDC contract needs correct copying, byte serialization,
types, keys and size, including native entries not projected by today's Java
map. Adding only get/has under the full Paper interface would hide missing
behavior. Reuse the real native CUSTOM_DATA state.

These become sequential prerequisites: canonical component bridge, complete
PDC view/container, then trim/tag/meta exposure. Runtime bridge payloads must
not accidentally become persistent numeric registry IDs; public item-byte
persistence has its own [format/version contract](item-persistence-plan.md).

The [canonical live item design](item-state-bridge-plan.md) selects immutable
native snapshots with explicit Java edits. Ordinary network serialization
is not a lossless substitute: it omits live bundle selection. Lease ownership,
atomic consumers and separate persistent item bytes all need their own proof.
The [complete persistent-data contract](persistent-data-plan.md) records the
type, copying, retained-view and public NBT-byte requirements for owner markers.

### Equipment events and dropped objects

Paper132 fires PlayerArmorChangeEvent from LivingEntity's tick-time equipment
diff, for players and the four humanoid armor slots only. The event is not
cancellable; its old/new stacks are independent Bukkit copies, including
non-null empty items. Mutating these copies does not write back to equipment.
Foton already has an owned equipment-diff result before attribute refresh and
tracker updates, so event delivery belongs there rather than in each click,
drag, hotbar or dispenser handler. Dispatch must occur without an inventory
or equipment lock held across plugin callbacks. Zelda schedules its cleanup
for the next task execution; that order still needs a real fixture.

The earlier audit incorrectly treated the drop event's cancellation-only
return as proof that item edits were lost. FotonItem.setItemStack already
reaches the actual ItemEntity by UUID, including pending spawns. This direct
write needs the canonical codec and runtime proof, not a duplicate returned
stack field. A separate source-verified mismatch remains: Paper cancellation
restores the entity's current, potentially listener-modified stack, whereas
Foton restores its pre-event clone. The pinned Paper call chain also confirms
PlayerDropItemEvent precedes ItemSpawnEvent on the same entity, before world
insertion; Foton currently reverses them. Cancelling the first Paper event
skips item-spawn entirely. Its restitution prefers the emptied main hand,
then a one-item similar merge, then addItem; Foton uses inventory.add directly.
The shown Paper drop method does not restore inventory when the later spawn
event rejects insertion. Preserve these distinct branches in the planned
player-drop lifecycle fix rather than just swapping two callbacks. Empty or
changed stacks, entity removal, full inventory and callers' final outcomes
still need explicit runtime fixtures.

### Piaf protection and tooltip flags

The native typed ATTRIBUTE_MODIFIERS component already feeds equipped-player
armor, toughness and knockback attributes, followed by damage and durability
handling. The established gap is the Java API and transport into that component,
not a demonstrated defect in the native damage formula. Paper's Attribute is
an interface and EquipmentSlotGroup a final predicate class; Foton's current
enum shapes and missing modern attribute fields do not satisfy that ABI.

Paper metadata reads the component patch, not all effective prototype values.
A chestplate whose only edit is a name therefore exposes no custom modifiers,
even though it still grants vanilla armor. Zelda copies only actual custom
entries, then adds its own keyed Piaf modifiers to the chest slot. Preserve
an explicitly empty modifier patch separately from absence/defaults. Adding
the same key on the same attribute throws in Paper; it is not replacement.

HIDE_ATTRIBUTES and HIDE_UNBREAKABLE change TOOLTIP_DISPLAY's hidden-component
set. They must not remove the effective modifiers or UNBREAKABLE component.
The later adapter needs clone/donor/collision/slot tests, then actual equipped
protection and durability comparisons, including unequip/re-equip and original
plastron restoration. Source-established native wiring alone is not that proof.

### Executed Paper attribute reference

One isolated Paper1.21.11 build132 run completed70 assertions with two expected
exceptions, then saved and stopped cleanly. Its fixture mirrors the pinned
Zelda Piaf modifier/enchantment/unbreakable operations for iron, diamond and
netherite donors. It proves the default-versus-empty patch distinction, exact
custom modifier keys/values/groups, clone and donor independence, duplicate
rejection, immutable getters, and tooltip-only hiding. Official item bytes
restore equal items with the tested components; deep independence after byte
restore was not tested by mutating the restored result.

Evidence is retained at
`%TEMP%/Foton-Armor-Paper-oracle-2c80113b96374e2d93da95d883340ca6/`.
Source/runtime/API inputs match the pinned item-state oracle. The original
wrapper exited1 only at its final hash check because Paper rewrote the mutable
server.properties file. A separate read-only post-check exited0, verifying
immutable inputs, raw assertions, isolated loopback configuration and clean
stop without rerunning the server. The initial configuration's hash, but not
its original contents, was retained; the report records that limitation.

These results do not establish equipped protection, damage, durability wear,
rendering, player events, Foton transport or unchanged Zelda gameplay.

## Bounded delivery and acceptance

1. Implement exact trim/tag/PDC-view/data-presence API behavior and real typed
   armor metadata/trim transport. Resolve official class kinds/descriptors,
   meta selection, clone/equality/clear behavior and prototype-versus-patch
   semantics before editing. Prove Java→native→Java and native persistence
   retain the item, not just an in-memory ArmorMeta field.
2. Carry custom modifiers, flags and relevant enchantments through the same
   authoritative item bridge. Compare actual equipped attributes, protection,
   toughness, knockback and durability for the Piaf elytra with Paper. Keep the
   plugin's material-specific additions as plugin behavior, not copied vanilla
   data in Foton foundations.
3. Provide full-fidelity item byte persistence for the Piaf backup and restore.
   Establish Paper's byte/NBT contract and version boundaries first; include
   PDC, trims, modifiers, enchantments, damage and opaque components. Do not
   silently add migrations for old Foton-specific data without demonstrated
   need; compatibility with the declared Paper input format is a separate
   requirement. Test recovery with an occupied slot and malformed backup.
4. Deliver the real armor-change event and verify writable dropped-item
   behavior at the native transition, with correct scheduling/cancellation
   and cleanup.
   Cover click/drag/shift/hotbar changes, cursor/offhand/ender chest/container/
   armor stand, other-player pickup, quit/rejoin and both death modes. No
   standalone event class without native delivery counts as support.
5. Run the unchanged plugin's complete workflow on both servers once its
   startup dependencies work. Compare component data, worn slots, effective
   protection, actual durability, drops, saved state and visible equipment.
   Observe native 26.2 and Via-translated 1.21.11 clients, including a second
   player's view. Restart must preserve the original item and the purchase
   while applying/stripping plugin ownership at the correct time.

The early slices use focused Paper-compiled fixtures and native tests; they
are not substitutes for step 5. No armor slice is certified by this audit.
