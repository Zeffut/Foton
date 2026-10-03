# Container transfer foundations for Paper compatibility

Source investigation: 2026-10-03, Foton Minecraft 26.2. This is a design and
scope record, not implementation or passing compatibility evidence.

The native brewing lock slice does not depend on this work. A complete
Paper Container/BrewingStand holder and general block/item transfer do.

## Runtime capture is not persistent encoding

Local `minecraft-src` defines the distinction in `ItemStackTemplate.java`
and `ItemContainerContents.java`: capturing a live item copies its item,
count and component patch without invoking its persistent codec. The codec
separately restricts persisted count to 1–99. Container capture preserves
internal holes and trims trailing empty slots; its 256-slot representation
bound is independent of an individual runtime stack count.

Foton's `ItemStackTemplate::from_stack` currently invokes strict persistent
construction and eager hashing. Merely making block-entity collection return
`Result` would expose failures, but would still reject runtime snapshots that
vanilla can capture. Fix that boundary before broadening collection.

Runtime capture must retain count 100 and persistent-invalid component values
without certifying them as valid for disk, untrusted input or placement. Keep
strict constructors and ingress validation. Empty source stacks follow the
real `ItemStack::is_empty` rule. A non-destructive shared container capture
must preserve source slots/patches, allow arbitrarily many trailing empty
slots, and report an occupied slot beyond the representation limit.

`ItemStackTemplate.create()` separately validates item size and may return an
empty stack. Capturing a runtime value therefore does not promise a lossless
capture→create→place round trip for every possible value. Transactions that
promise explicit failure must validate/materialize all items before mutation.

## Persistent hashing must fail explicitly

The existing stream template can have no cached persistent patch hash; its
public infallible `HashComponent` implementation can then panic. This is an
API precondition hazard, not a demonstrated remote crash. Inspected creative
ingress validates nested persistent data before returning a packet, and
container-click comparison already handles registered hash errors.

Before broad runtime capture, make persistent hashing explicitly fallible for
the template and its five recursive wrappers: `UseRemainder`,
`SulfurCubeContent`, `ChargedProjectiles`, `BundleContents` and
`ItemContainerContents`. No public direct infallible path may retain the panic,
invent a hash or report a partial hash as success. The dynamic registry hash
callback already returns `Result`; ordinary leaf `HashComponent` APIs need
not change. Removing the eager optional cache is the proposed correctness
baseline. Any later cache must not make equality construction-dependent.

Important preflight: the existing template depth guard covers stream/NBT
decoding and stream writing, not every source-side validation/hash traversal.
Unconditional recursive encode/redecode validation before every nested hash
also repeats work. A private checked-recursion context must cross template,
wrapper, patch and dynamic registry edges without resetting depth. Leaf and
recursive validation guarantees must remain intact. Verify the NBT dependency's
physical list/compound depth limits before equating codec acceptance with a
template-count limit. This exact interface/depth policy still needs an
implementation preflight; do not guess it from the constant 512.

The pinned dependency source is available: simdnbt 0.10.0's borrowed parser
rejects a push reaching 512 active frames, so 511 is its accepted maximum.
Compounds and lists of lists/compounds consume frames (including typed empty
lists); primitive lists do not. Current strict validation reparses each
component root, not one outer item root. The proposed checked traversal carries
both its hash and physical frame height internally, enforcing that height at
every component boundary. Actual nested custom-NBT leaf height must propagate:
a leaf accepted by itself can exceed the limit inside a template and recursive
component. Compare the height accounting against the pinned parser in tests;
this source-derived design is not yet an executed acceptance result.

## Complete transfer sequence

| Slice | Required result | Not established by this slice alone |
| --- | --- | --- |
| A — registry runtime boundary | Non-destructive runtime snapshots, strict persistent boundaries, checked fallible hashing and bounded traversal | Block-entity collection, destruction, placement or pick-block |
| B — authoritative collection and transactions | Current inventory overrides stored CONTAINER; complete collection errors reach a pre-mutation abort boundary; all destination values prepared before commit | Missing raw block-data and creative include-data routes |
| C — block/item transfer | Correct raw-data/component precedence, permissions/type checks, component-backed tag removal and real creative include-data capture | Java holder and BrewEvent acceptance without their separate fixtures |

Slice B must cover actual destructive callers, not just change a trait's
signature. The current shulker behavior drains `take_all()` before fallible
template conversion and skips rejected items. Prepare the complete output
without draining, then commit removal exactly once; failed preparation must
leave the block and every item intact. Test creative and survival separately.
`block_drops`, `default_block_drops`, behavior drop hooks and creative destruction
currently have no consistent error/abort channel. Missing loot context or zero
drops is not an acceptable substitute for collection failure.

Use one inventory-component owner. Collection overlays authoritative current
items on stored components, including when empty. Implicit application must
claim CONTAINER so stale inventory snapshots do not remain in leftover data.
Migrate existing bookshelf/shulker behavior-level restore hooks together with
that ownership; avoid duplicate restoration and a brewing-only workaround.

Slice C must preserve vanilla order. Block-item placement applies BLOCK_STATE,
then typed BLOCK_ENTITY_DATA under type/operator restrictions, then implicit
components, then `setPlacedBy`; components win overlapping raw fields. Foton's
raw-data stage is currently absent while leftover handling forgets that key.
Creative include-data pick currently lacks its whole transfer: save custom NBT,
strip component-backed fields, set typed BLOCK_ENTITY_DATA, overlay collected
components, then insert the item. An unused tag-removal hook is not support.

## Required evidence

Use real existing component fixtures, not invented registry data. Cover sparse
and boundary slots, unchanged repeated capture, set/removed patches, transient
components, runtime count 100 versus persistent rejection, all five nested
hash paths and valid HashOps parity. Bound deep direct/registered recursion and
verify recovery after errors; count visits instead of relying on timing alone.
Then prove failed shulker transfer changes no source slot/block or emitted
item, successful transfer commits once, and raw-data/component conflicts obey
the source-defined precedence and permissions. Keep current untrusted rejection
tests green. Each slice requires its own review and scoped tests before the
next depends on it; none is currently claimed complete.
