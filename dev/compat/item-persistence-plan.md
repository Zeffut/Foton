# Zelda chestplate backup: public item bytes

Source contract established against pinned Paper1.21.11 build132 and Zelda
Git object `3c324d63c502eb18242af7c771baa11ea1c2915c`, with Foton's native
MC26.2 source/codecs inspected separately. This plan is not a Foton persistence
pass. The [Paper armor oracle](zelda-armor-plan.md) proves only representative
official roundtrips.

## Actual format and use

ChestplateBackup writes a single serializeAsBytes result before Piaf flight.
A zero-length file is Zelda's own absent/AIR marker, not a Paper serialized
empty item. Loading nonempty bytes delegates to deserializeBytes; exceptions
produce null after logging. Failed import is a real recovery failure.

Paper encodes native ItemStack.CODEC through registry-aware NBT, adds integer
DataVersion from the running server, then writes GZIP-compressed compound NBT
with an empty root name. The item body is id, count and optional components.
Components encode persistent changes relative to the prototype, including
!-prefixed removal keys; transient components are omitted.

The public writer rejects null/empty items; the reader rejects null/zero-byte
input and newer DataVersion. Missing/wrong-type version defaults to zero.
Paper invokes its ITEM_STACK DataFixer before registry-aware item decoding;
missing/AIR id after conversion becomes empty. Changing a version number alone
cannot perform that schema conversion. PDC bytes are a separate format.

## Foundation and delivery gates

1. Strict same-target writer/reader over canonical native state. Foton already
   has persistent item/patch NBT codecs, but its save-time encoder can log and
   omit failed components: public backups must use strict fallible encoding.
   Bound recursive validation before traversing nested items/custom NBT. The
   existing Java FTON bytes are incompatible and incomplete; live lease ids,
   process epochs and transport projections must never enter durable output.
   Deserialization produces a fresh owned native transfer.
2. Same-version roundtrips of damage, enchantments, PDC, trims, custom modifiers,
   tooltip flags, opaque persistent values and component removals. Verify
   restored independence by mutation, not just different object identities.
   Preserve the specified omission of transient components.
3. Verified versioned ITEM_STACK upgrade support for declared older Paper
   backups, with real pinned input bytes; reject newer versions. No vanilla
   item DataFixer was found in the inspected Foton native sources. Until it
   exists, older-version import must fail explicitly, not relabel or parse
   old schemas as current. Output uses the actual MC26.2 target DataVersion,
   not a Paper1.21.11 number merely because Zelda links that API.
4. Invalid root/count/registry/component, missing/older/newer DataVersion,
   truncated GZIP, trailing-data, depth and decompression-size tests. Paper's
   inspected path uses unlimited heap accounting with a depth ceiling, not a
   small byte cap. No explicit post-root consumption check was found, which
   does not prove every trailing GZIP pattern is accepted. Establish runtime
   behavior and label stricter safety limits as restrictions. Failures must
   leave existing live items and backups untouched.
5. Unchanged Zelda flight/landing/quit/death/join/restart, including occupied
   chest-slot recovery without duplication or loss. Saved-state equality and
   native/Via clients' equipment observations are both required.

No earlier gate certifies the later ones. The [live carrier](item-state-bridge-plan.md)
temporarily rejects unsupported serialization; that safety boundary is not
a replacement for full Paper persistence.
