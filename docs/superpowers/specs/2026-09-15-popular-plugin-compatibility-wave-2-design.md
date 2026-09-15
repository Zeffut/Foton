# Popular Plugin Compatibility — Wave 2 Design

## Goal

Increase practical Paper/Bukkit plugin compatibility without signature-only stubs, fake Vanilla data, new tick-path work, or misleading compatibility claims. This wave is driven by two concrete gates: a fresh plugin-API build and the Zelda Civ plugin compiled and started against Foton.

## Baseline

- Foton branch baseline: `75f78b09351b3aa622dffd311831c7d61b566d38`.
- Shared-member compatibility: 2478/2487; the nine residuals are architectural boundaries and have no Zelda overlap.
- Zelda Civ baseline: 293 compiler errors at `2113484fc4bf8086fd268213bc64cf41968d2d19`.
- Zelda currently fails before `onLoad` when the external Simple Voice Chat API is absent. Foton must not provide fake Voicechat classes.

## Scope

### 1. Reproducible plugin-API build

`dev/build-plugin-api.sh` must work from a fresh checkout where ignored generated Rust registry sources do not yet exist. It may generate the required registry sources through the existing `foton-registry` build before running the Java generators. The Java source argfile must remain valid when the repository path contains spaces.

`update-minecraft-src.sh` must derive Minecraft 26.2 from the workspace version, pin the GitCraft revision used by the repository, use a temporary directory outside the checkout, and bound generation to the exact target version. This wave updates and tests the command construction but does not run the destructive source regeneration.

### 2. Existing entity truth exposed through Bukkit

Expose only state and mutations already implemented by Foton core:

- scoreboard tags;
- width and height derived from the existing bounding box;
- destination-filling `getLocation(Location)`;
- gravity state and mutation;
- silent state mutation;
- rotation mutation;
- rain-state query.

JNI work is request-driven. It must not add polling, caches, locks, or work to the game tick. Existing core validation and Vanilla semantics remain authoritative.

### 3. Exact API adapters

Add adapters whose complete behavior is already expressible through existing live APIs:

- equipment slot dispatch;
- `Plugin.reloadConfig()` and `Plugin.saveConfig()` contract exposure;
- world entity filtering by class;
- registry iteration from the existing stream;
- repeated shapeless ingredients;
- the precise `ResolvableProfile.Builder.build()` return type;
- Adventure `Key` implementation on `NamespacedKey`.

Adapters must delegate rather than duplicate state.

### 4. Backed modern aliases

Add modern attribute and potion names only as references to the existing keyed instances:

- attributes: `ARMOR`, `ARMOR_TOUGHNESS`, `KNOCKBACK_RESISTANCE`, `MOVEMENT_SPEED`, `SCALE`;
- potion effects: `HASTE`, `JUMP_BOOST`, `RESISTANCE`, `STRENGTH`.

Identity tests must prove aliases are the same objects/enum constants. No separate unbacked registry identities are permitted.

## Explicitly deferred

- MavenLibraryResolver and custom Paper loader execution;
- PaperAdventure/NMS conversion types;
- `Effect.getId()` until authoritative mapping and functional world effects exist;
- particle declarations without packet routing;
- displays, merchants, pathfinding, ray tracing, teleports with flags, scoreboards, item data components, trims, rich Adventure presentation, new event emission, and gameplay mechanics lacking verified foundations;
- any fake Simple Voice Chat classes.

## Verification

1. A copied checkout whose path contains spaces builds the plugin API without pre-existing ignored generated registry Rust.
2. Focused Java checks and Rust native tests cover the new bridge and adapter behavior.
3. Every planned Zelda diagnostic symbol disappears. The expected count is at most 244 after the entity/adapters tranche and at most 230 when all aliases apply, while acknowledging compiler cascade effects.
4. Runtime Zelda validation includes the real Simple Voice Chat dependency or records its absence as a plugin packaging/environment failure, never as a Foton API implementation.
5. `bash dev/ci.sh` passes on the exact final commit.

## Performance invariant

All new compatibility work runs only when a plugin invokes the API or during build/plugin loading. No new per-tick scans, allocations, JNI calls, or synchronization are introduced.
