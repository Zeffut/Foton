# Popular Plugin Compatibility — Wave 2 Design

## Goal

Increase practical Paper/Bukkit plugin compatibility without signature-only stubs, fake Vanilla data, new tick-path work, or misleading compatibility claims. This wave is driven by two concrete gates: a fresh plugin-API build and the Zelda Civ plugin compiled and started against Foton.

## Baseline

- Foton branch baseline: `75f78b09351b3aa622dffd311831c7d61b566d38`.
- Shared-member compatibility: 2478/2487; the nine residuals are architectural boundaries and have no Zelda overlap.
- Zelda Civ baseline: 392 compiler errors at `de3d12ac00a04f077b68c7537b36c689ac3679a2`, measured with a gate that rejects every residual `paper-api-*.jar` from the Maven classpath. The former 293-error measurement was contaminated by Paper and is invalid.
- Zelda at `ae06d49cb093a977ce2192bb31426a118d342c80` makes Simple Voice Chat genuinely optional and reaches `onEnable`. Its next failure is Foton-owned: `DriverManager` does not discover the SQLite JDBC driver shaded inside the plugin JAR.

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

### 5. Shaded JDBC driver discovery

Plugin code must be able to use a JDBC driver and `META-INF/services/java.sql.Driver` packaged inside its own JAR, as popular database-backed plugins do. Driver discovery and registration must occur through the plugin classloader at the correct lifecycle point, preserve classloader isolation, and avoid retaining plugin classloaders after disable/reload.

The regression fixture must contain a tiny in-JAR JDBC driver and service descriptor so the test remains deterministic and network-free. Foton must not special-case SQLite or add a global SQLite dependency.

### 6. Additional already-backed API

Add only the following low-risk contracts confirmed against the clean Zelda gate and existing runtime foundations:

- correct `Player` / `OfflinePlayer` / `AnimalTamer` hierarchy;
- legacy Bukkit game-rule handles backed by the current 26.2 rule names;
- `Tag.ITEMS_TRIMMABLE_ARMOR` backed by Foton's live item-tag bridge;
- `Enchantment.conflictsWith` backed by the registry's real bidirectional exclusive-set logic.

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
3. Every planned Zelda diagnostic symbol disappears. The clean baseline should fall from 392 to about 330 after entity/adapters/aliases and about 319 after the additional backed API, while acknowledging javac cascade effects.
4. A shaded, service-discovered JDBC fixture connects during plugin enable and releases its classloader-owned registration on disable/reload.
5. Runtime Zelda validation uses the version where Simple Voice Chat is truly optional and progresses beyond the SQLite connection that currently fails.
6. `bash dev/ci.sh` passes on the exact final commit.

## Performance invariant

All new compatibility work runs only when a plugin invokes the API or during build/plugin loading. No new per-tick scans, allocations, JNI calls, or synchronization are introduced.
