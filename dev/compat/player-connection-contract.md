# Live player connection contract for Via and Zelda dependencies

Recorded 2026-10-03 from the unchanged official ViaVersion 5.11.0 JAR
(`18d19e90fc9467d68128c076630ae8700449c901402a3ef421837ce006bc8cae`)
and current Foton code. This is a static prerequisite inventory, not an
implemented CraftBukkit layer or plugin certification. The real frozen join
run and its remaining warning are recorded in `zelda-civ-baseline.md`.

## ViaVersion's exact join reflection

`NMSUtil` derives its CraftBukkit package from the concrete
`Bukkit.getServer().getClass().getPackage().getName()`. With `foton.FotonServer`,
it therefore looks for `foton.entity.CraftPlayer`, not an independently chosen
conventional package. Its `JoinListener` requires this chain:

1. A publicly invocable zero-argument `getHandle()` declared on that class;
   an inherited method does not satisfy `getDeclaredMethod`.
2. The declared return type has a declared field whose type's simple name is
   `PlayerConnection` or `ServerGamePacketListenerImpl`. No superclass search
   occurs at this step.
3. That field type, or its superclass, has a field whose type's simple name is
   `NetworkManager` or `Connection`.
4. That field type has a declared field of the exact shared
   `io.netty.channel.Channel` class. Declaring it as `EmbeddedChannel` fails
   even when the value is a valid channel.

Field names are immaterial; duplicate candidate fields risk selecting the
wrong connection. At join, reflection operates on the event's actual Player,
not a separate adapter. The final channel must be open and contain Via's
`BukkitEncodeHandler`; Via obtains that handler's UserConnection, sets the
player UUID/name, calls `onLoginSuccess`, then sends its server-details message.

## Required native ownership, not reflection-only classes

Current `FotonPlayer` is final and retains UUID plus a session generation.
Join uses `commitLoginAttempt`, while Bukkit lookups construct player views.
`FotonViaChannel` separately owns an EmbeddedChannel retained by Rust's
`ViaTranslator`, created at TCP acceptance. No player/session-to-channel
association currently connects those two lifecycles.

The eventual adapter/factory must return real compatible player objects from
events and Bukkit lookups, preserving existing casts, identity and session
rules. The handle chain must lead to the same channel that Via instrumented
for that TCP connection. Creating another channel or looking up only a UUID
would associate the wrong protocol state on reconnect. Connection binding must
exist before PlayerJoinEvent and be released on failed admission and close;
a retained old player view must not resolve a newer session's connection.

Relevant boundaries are `FotonPlayer`, `FotonServer`, `EventBridge`,
`FotonViaChannel`/`FotonViaBridge`, Rust `via.rs` and `forward.rs`, transport
acceptance in `foton/src/lib.rs`, and core `JavaConnection`/player admission.
Design the real versioned internals together with Simple Voice Chat's server
and player handles; satisfying Via's first reflective class name alone does
not satisfy Zelda's companion or general Paper internals.

Required future tests include exact reflected descriptors, actual event/lookup
player types, connection identity, failed admission, disconnect/reconnect and
stale-view cleanup, plus real native/translated joins with the unchanged Via
JARs and no disabled-fixer warning. This contract does not by itself validate
packet injection, arbitrary Channel writes or Voice Chat UDP behavior.
