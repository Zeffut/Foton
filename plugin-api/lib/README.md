# Third-party libraries

The Bukkit API this server implements is written against Adventure, Brigadier
and a handful of ordinary Java libraries. These jars serve two roles: `javac`
uses them to build the API, and the optional `foton-plugin-runtime` release
asset installs the same pinned set beside `foton-plugin-api.jar`. Foton puts
that complete directory on the embedded JVM classpath so plugins can resolve
the server-provided API and its transitive types at runtime.

They are committed rather than fetched. The alternative makes a build step
depend on Maven Central being reachable, and this repository has just spent an
evening removing exactly that kind of dependency -- a check that passes or
fails for reasons that are not the code.

## Which versions, and why these

Every version below is the one `io.papermc.paper:paper-api:26.2.build.121-stable`
declares, read from its published POM and -- for the Adventure artifacts, whose
versions the POM leaves to a BOM -- from `net.kyori:adventure-bom:5.2.0`.

That matters more than it looks. A plugin is compiled against real Paper, so
the signatures it references are Paper's. Compiling our `org.bukkit` against a
different Adventure produces classes that link and then fail somewhere else,
which is the same failure `Levelled` caused when our spelling of a Bukkit
interface diverged from Bukkit's.

The first version of this directory held Adventure 4.26.1 beside
`adventure-text-logger-slf4j` 5.2.0 -- a logger built against Adventure 5 on
top of an API from Adventure 4. It compiled. That is exactly why the set is now
pinned by digest and checked rather than trusted for compiling.

`examination-api` and `examination-string` are gone rather than updated:
Adventure 5 dropped the dependency, and `adventure-api:5.2.0` names neither.

Guava's POM has four real runtime dependencies: `failureaccess`, `jspecify`,
`error_prone_annotations` and `j2objc-annotations`. They are pinned here even
though the latter three are mostly annotations: `jdeps` proves Guava and
Adventure reference their classes. The POM also names the deliberately empty
`listenablefuture:9999.0-empty-to-avoid-conflict-with-guava` compatibility
artifact. It contains no classes and is intentionally excluded because Guava
itself provides `ListenableFuture`.

JOML's Maven POM is empty, but its published Gradle module metadata declares
`kotlin-stdlib-jdk8:1.8.20`; following that metadata and its POMs closes the
runtime with `kotlin-stdlib-jdk7`, `kotlin-stdlib` and
`kotlin-stdlib-common`. This is why closure is audited from both metadata
formats and bytecode rather than from POMs alone.

The five Netty modules come from Minecraft 26.2's generated
`minecraft-src/minecraft/src/dependencies.json`, not Paper's API POM. Paper's
internal `ChannelInitializeListener` surface exposes a Netty `Channel`, and
ViaVersion links that signature during bootstrap. Only the Java transports
needed by the in-process compatibility channel are included; native epoll and
kqueue transports are deliberately outside this portable runtime set.

`dev/fetch-plugin-api-libs.sh` holds the SHA-256 of each jar. The build runs it
with `--check`, so a jar that is edited, swapped or added is a build failure
rather than a surprise in the bytecode. Run it without `--check` to download a
missing or changed jar from Maven.

## Licenses

Every license was read from the artifact itself or from the project's published
POM, not from memory and not carried over from the previous version.

| Jar | Version | License | Verified from |
|---|---|---|---|
| adventure-api | 5.2.0 | MIT | Maven Central POM |
| adventure-key | 5.2.0 | MIT | Maven Central POM |
| adventure-text-logger-slf4j | 5.2.0 | MIT | Maven Central POM |
| adventure-text-serializer-plain | 5.2.0 | MIT | Maven Central POM |
| annotations (JetBrains) | 26.1.0 | Apache-2.0 | Maven Central POM |
| brigadier | 1.3.10 | MIT | `LICENSE` in Mojang/brigadier |
| gson | 2.14.0 | Apache-2.0 | POM inside the jar |
| guava | 33.6.0-jre | Apache-2.0 | `META-INF/LICENSE` inside the jar |
| failureaccess | 1.0.3 | Apache-2.0 | `META-INF/LICENSE` inside the jar |
| jspecify | 1.0.0 | Apache-2.0 | Maven Central POM |
| error_prone_annotations | 2.47.0 | Apache-2.0 | Maven Central POM |
| j2objc-annotations | 3.1 | Apache-2.0 | Maven Central POM |
| joml | 1.10.8 | MIT | Maven Central POM |
| kotlin-stdlib-jdk8 | 1.8.20 | Apache-2.0 | Maven Central POM |
| kotlin-stdlib-jdk7 | 1.8.20 | Apache-2.0 | Maven Central POM |
| kotlin-stdlib | 1.8.20 | Apache-2.0 | Maven Central POM |
| kotlin-stdlib-common | 1.8.20 | Apache-2.0 | Maven Central POM |
| slf4j-api | 2.0.17 | MIT | `META-INF/LICENSE.txt` inside the jar |
| snakeyaml | 2.2 | Apache-2.0 | POM inside the jar |
| netty-common | 4.2.15.Final | Apache-2.0 | Netty repository `LICENSE.txt` at tag `netty-4.2.15.Final` |
| netty-buffer | 4.2.15.Final | Apache-2.0 | Netty repository `LICENSE.txt` at tag `netty-4.2.15.Final` |
| netty-transport | 4.2.15.Final | Apache-2.0 | Netty repository `LICENSE.txt` at tag `netty-4.2.15.Final` |
| netty-resolver | 4.2.15.Final | Apache-2.0 | Netty repository `LICENSE.txt` at tag `netty-4.2.15.Final` |
| netty-codec-base | 4.2.15.Final | Apache-2.0 | Netty repository `LICENSE.txt` at tag `netty-4.2.15.Final` |

Brigadier is the one that cannot be read from the artifact: version 1.3.10's
jar carries no license file, and its published POM declares no `<licenses>`
block. The row above cites Mojang's repository because that is the only place
the license is actually stated.

`com.mojang:logging` was here too and has been removed. Nothing in
plugin-api/src imports it, and unlike Brigadier it is not published under a
license anyone could point at -- it reaches libraries.minecraft.net as part of
Minecraft's own dependency set. An unused jar is not worth a license question.

## When a version changes

Take the new version from paper-api's POM rather than from what is newest.
Replace the jar, update its SHA-256 in `dev/fetch-plugin-api-libs.sh`, and
update the row above including where you read the license. A row that says a
license without saying where it was read is a row that will be wrong
eventually.

`dev/plugin-runtime-test.sh` checks the whole pinned classpath with `jdeps` and
executes a Guava `SettableFuture`. This catches both a missing transitive jar
and a bundle that merely looks complete by filename.
