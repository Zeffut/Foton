# PrepareItemEnchantEvent compatibility slice

Zelda Civ `3c324d63` cancels this event unless the enchanter has
`zeldaciv.bypass`. Foton now emits it from the real enchanting menu after rolling
offers, before those offers can be selected. Cancelling clears all three costs
and clues. A nonempty, non-enchantable input starts cancelled, as on Paper, and
listeners can uncancel it and supply offers. Empty input does not fire.

The event carries the actual player, table location, bookshelf bonus, item and
lapis, with writable offers and a typed `EnchantmentView`. `InventoryView` is an
interface, matching the Paper 1.21.11 interface kind and the method descriptors
exercised by the fixture, not its entire API surface. The player inventory is
unlocked during dispatch; table input and offer changes are applied after the
synchronous callback. Seed/offer writes outside callbacks operate on the real
menu without rerolling. A top-slot write reruns preparation.

Each view is bound to the existing process-unique menu identity, not the cyclic
network container id. Reads, item/offer/seed writes, and close requests from an
expired view fail explicitly; they cannot operate on a later table.
Retained view and top-inventory references use the current prepare context for
the same player and menu instance. Deferred closes retain that identity when a
listener queues a replacement menu.

The vanilla roll now selects its displayed clue from the rolled list using the
same RNG after selection, and the per-slot seed addition wraps as a Java int
before widening. Wire data slots narrow integers to shorts; the view retains
full-width integers.
Creative enchanting keeps vanilla's level charge and seed renewal while waiving
lapis and the level requirement.

Sources checked:

- Local MC 26.2 `EnchantmentMenu.java`.
- [Paper 1.21.11 prepare event](https://github.com/PaperMC/Paper/blob/ver/1.21.11/paper-api/src/main/java/org/bukkit/event/enchantment/PrepareItemEnchantEvent.java).
- [Paper enchantment-menu patch](https://github.com/PaperMC/Paper/blob/ver/1.21.11/paper-server/patches/sources/net/minecraft/world/inventory/EnchantmentMenu.java.patch).
- [Paper live view](https://github.com/PaperMC/Paper/blob/ver/1.21.11/paper-server/src/main/java/org/bukkit/craftbukkit/inventory/view/CraftEnchantmentView.java).

Verification:

- `cargo test -p foton-core enchant` covers actual event cancellation, writeback,
  an initially cancelled stone input, unlocked inventory, seed overflow,
  expiration after closing/reopening a table, replacement-safe deferred close,
  and creative level costs.
- `bash dev/prepare-enchant-test.sh` compiles its listener against the pinned
  official Paper 1.21.11 API, loads it in Foton's plugin host, and dispatches the
  actual Java event. Assertions exercise `invokeinterface`, table/player/view
  identity, mutable offers, inventory writeback, and the full-width seed,
  including references retained from an earlier prepare of the same menu.
- Java API checks and JNI descriptor checks cover the bridge.

This is not certification of all enchanting APIs. Retained views after closure
do not preserve Paper's detached menu object, and writes during preparation are
applied on callback return rather than recursively firing `slotsChanged` inside
the listener. `EnchantItemEvent` (the final purchase hook) is a separate missing
slice. None of those APIs are represented by successful no-op implementations.

The pre-existing missing `InventoryView` descriptors remain unimplemented:
`getCursor`, `setCursor`, `getSlotType`, `open`, `setProperty`, `title`,
`getOriginalTitle`, `setTitle`, `getMenuType`, and its `Property` type. This
slice does not claim linkage support for those calls.
