# Zelda armor compatibility

Explicit user requirement, 2026-10-03. Source audit uses the unchanged Zelda
Git object `3c324d63c502eb18242af7c771baa11ea1c2915c`, not its dirty working
checkout. Paper 1.21.11 build 132 is the plugin behavior/ABI oracle; Foton's
native mechanics use Minecraft 26.2 source and extracted data. This document
records requirements and gaps, not passing runtime evidence.

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
| Dropped items | Current drop-event response carries cancellation but not Zelda's changed Item entity stack |
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
persistence has its own explicit format/version contract.

The [canonical live item design](item-state-bridge-plan.md) selects immutable
native snapshots with explicit Java edits. Ordinary network serialization
is not a lossless substitute: it omits live bundle selection. Lease ownership,
atomic consumers and separate persistent item bytes all need their own proof.
The [complete persistent-data contract](persistent-data-plan.md) records the
type, copying, retained-view and public NBT-byte requirements for owner markers.

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
4. Deliver the real armor-change event and writable dropped-item behavior at
   the native transition, with correct scheduling/cancellation and cleanup.
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
