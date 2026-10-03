# Canonical live item state for Paper plugins

This is an implementation design, not a compatibility pass. It supports Zelda
armor trims and the Piaf chestplate workflow without changing the plugin.

## Why the current projection is insufficient

The current inventory wire rebuilds an item from its prototype and selected
Java metadata. Native GLIDER additions and explicit DAMAGE removals can be
lost after an unrelated edit. The separate opaque NBT string is not the typed
component patch. Public item-byte serialization is independently incomplete.

A native network-patch envelope alone also loses data: BundleContents stores
the selected item index outside its normal network/persistent codecs. That
index affects extraction, so equality of encoded patches is not enough.
Persistent codecs intentionally omit transient components as well.

## Selected live carrier

Use an immutable native ItemStack snapshot retained by an opaque lease. The
existing semantic clone preserves typed values, removals, transient fields,
nested templates, bundle selection and opaque NBT without codec reconstruction.
Current components are owned values/frozen registry references; future native
component extensions must preserve snapshot value semantics too.

Java stacks, detached metadata and clones share one immutable lease referent,
but have independent mutable edit journals. One Cleaner releases the native
entry after all Java owners disappear. Owning JNI transfer/mutation/event DTOs
keep that referent alive across construction and response parsing; a token in
a String alone does not establish ownership. Failed JNI construction rolls
back admission. Lookup takes an Arc before releasing the table lock; cleanup
is idempotent and never invalidates an operation already holding that Arc.

The epoch is process-scoped while the registry is immutable. Ordinary bind,
plugin disable and reload must not revoke cross-plugin item copies. Terminal
cleanup waits for existing callback/deferred teardown quiescence through the
host's invocation lifecycle, without sleeping or polling the game tick. Close
admission before retiring entries; late Cleaner calls are harmless.

Native candidates on plugin-created threads are not necessarily tracked by
the Java host. Closing admission must therefore signal a separate async drain:
the outer server shutdown waits for admitted item operations to finish before
world saving. Final permit release must wake this wait without a lost race,
and no lock or game tick may wait on a callback. Constructor failures and
deferred/reentrant shutdown use the same boundary. Synchronous Drop can only
perform best-effort cleanup; embedders must await the explicit async shutdown
contract before their own world teardown. A failed Java close handshake must
be reported and native admission sealed, not disguised as a successful drain.

## Edits, errors and resource limits

Keep unchanged, SET, REMOVE and RESET-to-prototype distinct. Hydration is not
an edit. A same-value book setter can intentionally clear filtered content,
so comparing getters before/after is not a complete edit journal. Preserve
unrelated native components and full custom data on each explicit edit.
Paper's donor-meta, null-meta, type-change and empty-state behavior must be
observed before wiring those transitions, not inferred from native emptiness.

The first carrier slice transports the mutation groups already supported by
the native codec. Existing Java-only attribute, flag and subtype setters are
not proof of native support: attempted unsupported edits must fail explicitly
before commit, while untouched native values remain intact. Their actual
editing implementations remain separate armor/component work. This temporary
limitation must be reported, never counted as complete ItemMeta compatibility.

Foton currently runs plugin onLoad before registry initialization to collect
declarations. Canonical item queries requiring that registry must fail clearly
at this stage instead of panicking or inventing prototype values. A fallible
registry-readiness check makes this safe but does not make these early calls
Paper-compatible; bootstrap ordering/readiness remains a separate gap. Do not
initialize the registry prematurely merely to hide that dependency.

Stage complete item/bulk/event responses before committing any mutation.
Merchant offers, death drops, crafter results/remainders and inventory arrays
must not silently skip malformed elements or partially update. Missing/stale
leases and invalid edits are explicit errors, never empty-item substitutes.
Native component semantics, not transport IDs or encoded byte order, determine
item similarity. Existing book/PDC roundtrip regressions remain required.

Admission has configurable entry limits and existing bounded transport. A
live lease is never evicted to satisfy a new read; exhaustion fails before
cloning/mutation. Permits last until the final native operation guard releases
the snapshot, not just until its lookup entry is removed. Java clones sharing
one lease do not consume another native entry. In-flight materialization has
separate bounded admission so retained-capacity pressure need not prohibit
every update of an existing item.

These are cardinality/transport bounds, **not a hard total RAM bound**. Large
values and delayed Java GC remain operational memory risks. Do not invent a
serialized-size multiplier or widen every registry component with an unproven
heap estimator. Borrowed traversal must check actual recursive item/text/NBT
edges before recursive clone; choose admission depth from verified call-stack
tests and document any restriction rather than calling it a vanilla constant.

## Persistence is a separate boundary

Never put epoch, lease IDs, native pointers or runtime numeric registry IDs in
public item/meta bytes, configuration serialization or saved plugin data.
Those formats need complete, versioned durable semantics of their own. A
live carrier is not a fix for Zelda's Piaf backup; full persistent item bytes
and restore/restart tests remain mandatory before that workflow can pass.
Any unsupported serialization must fail explicitly rather than silently lose
newly retained components.

The first carrier slice uses a conservative boundary: every carrier-backed
item/meta serialization is rejected, even when its Java projection looks
simple. This intentionally blocks more cases than a future complete codec
will need to. No-carrier legacy formats remain only for fields they actually
represent; unrepresented PDC, subtype or opaque state must also fail instead
of becoming an empty map or incomplete backup. This is a temporary compatibility
gap, not a substitute for Paper persistence.

## Acceptance sequence

1. Native lease ownership, clone fidelity/depth, admission pressure, rollback,
   cleanup races and terminal lifecycle; actual JVM ownership tests.
2. Complete Java edits and transactional native consumers, including official
   Paper-observed meta transitions and existing book/PDC regressions.
3. Complete PDC container/view and typed trim/armor adapters over that state.
4. Durable chestplate bytes, custom attributes and real armor/drop events.
5. Unchanged Zelda scenarios, save/restart and two-client observation on native
   and Via-translated connections. No earlier slice substitutes for this pass.

## Executed Paper transition oracle

One isolated Paper1.21.11 build132 run completed111 observations and97 verifier
checks, with16 deliberately tested exceptions and clean save/shutdown. All
input hashes remained unchanged. Source, logs, verifier and manifests are at
`%TEMP%/Foton-ItemState-Paper-oracle-4c362e4a989047a987e0cc129da8bfc1/`.
Bootstrap/runtime/API identities match the pinned baseline. This is Paper
reference evidence, not a Foton pass or an exhaustive material conversion table.

- No-edit getItemMeta/setItemMeta preserves GLIDER and explicit DAMAGE removal.
  Null metadata resets the complete patch to the item's prototype.
- Captured donor metadata remains independent of later source changes and
  replaces recipient metadata as a whole. The tested armor donor applies to
  diamond chestplate, but refuses stone and leather chestplate unchanged.
- Changing iron to diamond retains count and patch. Changing next to stone
  removes a redundant DAMAGE removal; returning to iron then exposes default
  DAMAGE. Rebase/sanitize at each actual type transition.
- setAmount(0) publicly masks the item and PDC as empty, but later restoring
  count revives its original type and complete metadata. setType(AIR) instead
  destroys them; subsequently setting iron type creates a default count1 item.
- Retained PDC views follow applied owner changes, never unapplied metadata
  copies, and remain independently bound on clone. Native transfer may encode
  an empty slot while the detached Java object still retains latent state.

The same run pins complete PDC primitive/list/copy and named-root byte cases,
including modified UTF, duplicate names, trailing input and clear-before-parse
failure behavior. Intentional overlong-string serialization failure was
isolated and matched; it is not permission to truncate canonical live state.
