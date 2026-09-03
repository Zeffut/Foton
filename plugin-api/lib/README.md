# Third-party libraries

The Bukkit API this server implements is written against Adventure, Brigadier
and a handful of ordinary Java libraries. These jars are here so that `javac`
can resolve those names; nothing in them ships inside the server binary, and
Foton does not call into any of them at runtime.

They are committed rather than fetched. The alternative makes a build step
depend on Maven Central being reachable, and this repository has just spent an
evening removing exactly that kind of dependency -- a check that passes or
fails for reasons that are not the code.

Every license below was read from the artifact itself or from the project's
published POM, not from memory.

| Jar | Version | License | Verified from |
|---|---|---|---|
| adventure-api | 4.26.1 | MIT | Maven Central POM |
| adventure-key | 4.26.1 | MIT | Maven Central POM |
| adventure-text-logger-slf4j | 5.2.0 | MIT | Maven Central POM |
| adventure-text-serializer-plain | 4.26.1 | MIT | Maven Central POM |
| examination-api | 1.3.0 | MIT | Maven Central POM |
| examination-string | 1.3.0 | MIT | Maven Central POM |
| annotations (JetBrains) | 26.0.2-1 | Apache-2.0 | Maven Central POM |
| brigadier | 1.0.18 | MIT | Mojang/brigadier LICENSE |
| gson | 2.11.0 | Apache-2.0 | POM inside the jar |
| guava | 33.3.1-jre | Apache-2.0 | META-INF/LICENSE inside the jar |
| slf4j-api | 2.0.18 | MIT | META-INF/LICENSE.txt inside the jar |
| snakeyaml | 2.2 | Apache-2.0 | POM inside the jar |

`com.mojang:logging` was here too and has been removed. Nothing in
plugin-api/src imports it, and unlike Brigadier it is not published under a
license anyone could point at -- it reaches libraries.minecraft.net as part of
Minecraft's own dependency set. An unused jar is not worth a license question.

## When a version changes

Replace the jar and update the row above, including where you read the license.
A row that says a license without saying where it was read is a row that will
be wrong eventually.
