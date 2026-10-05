# Zelda compatibility checkpoint — 2026-10-03

The user requested stopping further implementation, stabilizing work already in
progress, committing it and pushing the working branch. This is a development
checkpoint, not completed Paper/Zelda compatibility or a release certification.

## Implemented in the current item-state slice

- Immutable native item snapshots, shared Java lifetime ownership and separate
  edit journals; checked component mutations and explicit admission failures.
- Owning transfers for migrated player/ender/mount/entity/block-item inventories,
  world drops and merchant offers, with complete batch staging before mutation.
- Merchant positive-patch requirements and original immediate ingredient display;
  full supported offer scalars, including experience reward across native saves.
- Source-derived metadata applicability, donor isolation, recipient stack-only
  opaque data preservation, and explicit refusal of unsupported conversions.
- Live menu read capabilities and retained deferred item batches, including
  differently sized full-replacement rejection and unlocked terminal disposal.
- Explicit persistence/readiness/unsupported-edit restrictions instead of
  silently discarding native state. These restrictions are not API parity.

The source and acceptance boundaries are detailed in
[the canonical item design](item-state-bridge-plan.md) and the README.
The snapshot cardinality default4096, aggregate traversal depth128 and four
nested-template ceiling are operational restrictions, not vanilla limits or a
hard memory guarantee.

## Verification at the requested stop

The initial source checkpoint is `95dbffa38`. Its focused checks passed:
60 player-inventory tests,52 plugin tests including the real embedded JVM,
33 trading tests and the Java API checks. The subsequent review found two
important defects: silently ignored unsupported metadata removals and accepted
empty merchant results that could not be read back. Both have been corrected in
`dcce14996` with regression coverage; the corrected52-test plugin suite and Java API checks
passed (43 existing Java warnings remain).

The first full CI attempt was not green: strict lint found an expression now
corrected, and a key-file permission test ran on a Windows temporary filesystem
that cannot represent the expected Unix permissions. The temporary-directory
configuration was corrected without weakening the security test.

The user's repeated request to stop and push quickly ends further validation.
No completed green full CI or independent re-review of the final corrections is
claimed. Resume those checks before promoting this development checkpoint.
The final trading rerun was interrupted during compilation; the queued release
Clippy rerun was not started. No test/server/compiler process was left running.
The5715 tests enumerated across20 targets are a test inventory, not evidence that
all tests were executed successfully on the final commit.

## Reference evidence, not Foton certification

Pinned Paper1.21.11 build132 reference fixtures completed independently:

- Item-state transitions:111 observations,97 verifier checks,16 expected exceptions.
- Armor/Piaf metadata:70 assertions,2 expected exceptions. No combat or rendering proof.
- Merchant costs:28 assertions,4 expected exceptions, including the actual stream codec.

Each used a fresh world in a private loopback-only network namespace. No Zelda
source or plugin JAR was altered. Raw fixtures, inputs and logs remain local
test evidence; proprietary plugin binaries are not part of this checkpoint.

## Required continuation

1. Complete owning item transports for event requests/responses, initial custom
   menu contents and instance-qualified enchanting inventory access. Validate
   the whole response before changing cancellation, messages or item arrays.
2. Run genuine server/player/plugin menu reentry and outer-host shutdown tests.
   Unit and DTO tests do not establish that end-to-end behavior.
3. Complete PDC view/container, trim/tag/armor metadata, attributes/tooltip flags,
   equipment/drop lifecycle and versioned durable chestplate backup.
4. Execute unchanged Zelda armor/ornament/Piaf scenarios through equip, death,
   reconnect and restart, including two-client native/Via observation.

Existing startup and network-internal gaps remain separately tracked; a working
client join or successful component test does not certify an unchanged plugin.
Embedders must await the explicit shutdown contract before tearing down worlds.
Synchronous Drop does not guarantee native quiescence; inherited Java teardown
can itself wait for invocations, so Drop is not universally nonblocking.
See [armor requirements](zelda-armor-plan.md),
[item persistence](item-persistence-plan.md) and
[the measured Zelda baseline](zelda-civ-baseline.md).
