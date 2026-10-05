# Complete Paper persistent-data contract

Prerequisite to Zelda armor ownership markers and the reusable tile/item API.
This records findings from the pinned Paper 1.21.11 build132 API and server
bytecode. It is not a Foton implementation or a runtime-equivalence result.

## One typed store

PersistentDataType is an interface with primitive/complex types and adapter
conversion methods, not an identity token. Store actual typed NBT primitives.
BOOLEAN uses Byte as its primitive; BYTE and BOOLEAN therefore match the same
stored tag. Numeric tag kinds remain distinct. `has(key,type)` checks the
primitive kind without invoking the complex conversion. `get` returns null
for absence but throws on an incompatible present primitive; getOrDefault
does not suppress conversion/type failures.

Support all official primitive arrays, nested containers, container arrays
and ListPersistentDataType/provider methods. Copy mutable arrays at input
and output. Nested containers returned by get are detached, not live views.
List adapters have observable lazy-conversion/mutator behavior; verify it
against the pinned fixture rather than replacing every result with an
immutable list. Legacy TAG_CONTAINER_ARRAY's permissive scalar-list handling
differs from LIST.dataContainers and needs a separate regression.

The full raw compound is authoritative, including native values not exposed
by the old STRING/BYTE/INTEGER projection. getSize counts raw entries; getKeys
only returns names convertible under Paper's key rules, so those counts can
differ. A complete implementation must not maintain conflicting projected and
opaque maps. copyTo preserves unrelated target keys and follows replace for
collisions; copy every raw entry, not only keys visible through getKeys.

## Views and mutation ownership

A retained ItemStack PersistentDataContainerView reads its current owner.
Unapplied metadata copies do not affect it; applied metadata and handle/type
changes do. Each cloned item has its own owner-bound view. The view is not a
mutable PersistentDataContainer disguised by a different return descriptor.
Container getKeys returns a mutable detached set; stack view getKeys returns
an unmodifiable detached set. Arrays and nested values remain detached.

Standalone containers and adapter contexts work without a live server lookup
for each primitive operation. Meta clones need independent mutable stores
with structural equality. The canonical item bridge supplies and consumes
complete CUSTOM_DATA/PublicBukkitValues and explicit mutation notifications.
Native tile PDC is a separate BlockEntityBase field with its own load/save and
client-filter lifecycle, not an item-component substitute.

## Public bytes are not the private bridge format

serializeToBytes writes uncompressed big-endian **named-root NBT**, with an
empty root name (initial bytes 0A 00 00). Names and string values use Java
modified UTF-8. No Foton header, network-NBT root omission, JSON or gzip is
equivalent. Tag ordering is not canonical; compare decoded values when order
is unspecified. A focused complete Java NBT codec or vetted compatible
dependency is required; there is no existing Java NBT implementation in the
current API runtime. Do not add incomplete byte methods just for linkage.

The pinned public reader has quirks that are separate from transactional
inventory mutation: clear=true clears before parsing, so malformed input
leaves an empty container; clear=false retains old contents on parse failure.
Successful reads merge incoming keys, with incoming collisions winning.
The root name is ignored and trailing bytes are accepted. Exceptions depend
on failure location; do not promise all malformed input throws IOException.
The pinned writer can log an overlong modified-UTF value and write an empty
string. Record observed behavior and disclose any intentional safety change.

Private JNI imports may still require stricter complete parsing and atomic
slot commit. They must not silently redefine these public reader semantics.
The public byte format must never contain process lease IDs or registry wire
identities. Verify native simdnbt interchange, duplicate keys, modern mixed
list representation, raw float values and modified UTF explicitly.

### Executed numeric and list reference cases

A standalone probe of the pinned Paper132 NBT classes passed 36 assertions
under Java21. It started no server or world and does not test Foton. Final
source/classes are retained in
`%TEMP%/Foton-PdcNbt-MicroOracle-6ab2eded6cce4055a916956a1a0561f7/`;
the controller's direct captured rerun is recorded in the implementation
ledger, separately from the probe's earlier transcribed attempt notes.

- Float/double `valueOf` and NBT reads normalize negative zero to positive
  zero; direct tag constructors retain its sign. Direct negative-zero tags
  therefore compare differently before and after serialization/readback.
  NaN payload variants compare/hash equally and write canonical NaN bits.
- Modern mixed lists wrap non-compound entries. Genuine sole-empty-key
  compounds require an extra wrapper on write so one unwrapping step on
  read preserves the original compound. A supplied physical single wrapper
  instead decodes to its scalar value; raw wrapper shape is not a semantic
  identity that can be copied blindly into the Java typed model.
- Empty lists normalize their declared element kind to END after read,
  including the tested unsupported kind99 with count0. This is not evidence
  that unknown nonempty list types are accepted.

These cases supplement, not replace, the public PDC server oracle. The raw
simdnbt parser accepts some invalid modified-UTF bytes, but the current
CustomData constructor normalizes its compound and rejects those bytes.
Do not manufacture an unreachable malformed PDC item to claim an ingress bug.
Raw import helpers must still reject unreadable strings rather than turn them
into empty Java strings. Valid overlong CustomData strings are a separate,
reachable case: the live snapshot can retain them even when the legacy Java
projection must refuse to materialize them without loss.

## Delivery gates

1. Complete type/adapter/view declarations and standalone typed store/codec,
   with official-Paper differential fixtures for all methods and ownership.
2. Integrate full custom data and dirty notifications with the canonical item
   bridge, then attach retained owner views with meta/type/empty/clone tests.
3. Bind the same complete contract to authoritative tile snapshots/live state
   without leaking PDC to ordinary client packets.
4. Run Zelda ownership cleanup, Piaf backup/restore and restart scenarios;
   isolated PDC tests alone are not armor or plugin certification.
