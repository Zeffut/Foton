# Integration compatibility boundaries (2026-10-05)

These are explicit integration choices, not a universal Paper certification.

## Plugin lifecycle and API

- Required BOOTSTRAP provider failure (loader, bootstrap or command callback)
  rejects its transitive required consumers before SERVER construction. Optional
  and unrelated plugins survive. Cycle recovery changes ordering only: callbacks
  that already ran in a recovered cycle cannot be undone, but their unpublished
  loaders/commands are retired if a required provider subsequently fails.
  This deliberately supersedes the incoming failed-provider tolerance.
- Bootstrap command callbacks remain pre-instance and once-only. Their trees
  are seeded, not callbacks replayed, on each instance enable. Existing instance
  early/late handlers, generation retirement and scheduler barriers remain.
- An explicitly null `PluginBootstrap.createPlugin` result retains Foton's
  ordinary main-constructor fallback in the same owning loader.
- `PluginProviderContext.getLogger()` retains the old `org.slf4j.Logger` JVM
  descriptor through a covariant interface bridge, alongside `ComponentLogger`.
  The ABI regression compiles its caller against the checked-in interface from
  parent `7f6a4cc42`, then runs those unchanged bytes against the new API.
- `LifecycleEventHandlerConfiguration` now has the incoming/Paper **interface**
  shape. The old Foton concrete constructor and `handler()` accessor are not
  binary or source compatible. Recompile callers to use the event type's
  `newHandler(handler)` factory (or direct lifecycle-manager registration), and
  retain their own handler reference if needed. No classloader rewrite or dual
  class/interface ABI is provided.
- The validated cached Kotlin serialization JSON declaration still adds its
  matching JVM core companion. Uncached declarations use the Maven graph
  resolver. Automatic `paper-libraries.json` remains a Foton-only adapter when
  no custom loader is declared; a custom loader owns that resolution exclusively.

## Incremental alias retirement and dependency generations

Only registered identities participate in live lookup. Retiring the selected
alias provider does not promote a previously unselected live competitor, even
on subsequent discovery. A new discovery batch can explicitly load a replacement
generation and its new consumers; fresh-batch deterministic selection is unchanged.

Required dependency admission and future dependency class resolution use the
provider generation prepared for each consumer, including same-batch AFTER/OMIT
providers. An initially absent Bukkit soft dependency or optional Paper SERVER
joined-classpath dependency can acquire its first registered provider later.
The binding occurs when that provider's selected identity and loader are
registered, before construction/publication, not at the consumer's first class
lookup. A preparation that fails before loader registration is not availability.
Once bound, an expired weak reference remains a retired binding: neither a
replacement generation nor a surviving unselected alias competitor can take over.

Required consumers fail the existing lifecycle validation when their bound
provider is retired; optional consumers can remain but cannot resolve new classes
through that retired binding. Classes already resolved by the JVM are not
retroactively unloaded.

## Player saves: format 13, no automatic migration

Both integration parents labeled incompatible positional player layouts as
storage/payload version 12: one omits scoreboard tags, the other inserts them
between `on_ground` and `fall_flying`. There is no reliable discriminator.

The merged tags-bearing layout is version **13**. All version-12 player files
are refused before decompression/schema interpretation. Domain-load checks
also reject old primary/backup headers before any recovery rename or overwrite.
Failure remains an error through startup/login, not a new empty player. Existing
files are not converted, deleted or reset. Operators must preserve their saves
and use a separately reviewed, provenance-aware migration before upgrading a
world containing version-12 player data; this change supplies no such migration.

## Canonical metadata corrections

Rich lore takes precedence over its plain fallback during hydration. Editing
it preserves its styles/children/hover and unrelated snapshot data. An explicit
empty banner-pattern SET is a typed empty component, distinct from REMOVE and
RESET, including on a non-prototype item carrying that component. These fixes
extend no unsupported conversions, item persistence or block-state capability.
