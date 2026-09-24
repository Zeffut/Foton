#!/usr/bin/env bash
# Prove the shipped plugin classpath is closed and executable, not just present.
set -euo pipefail
cd "$(dirname "$0")/.."

command -v javac >/dev/null 2>&1 || { echo 'javac is required' >&2; exit 1; }
command -v java >/dev/null 2>&1 || { echo 'java is required' >&2; exit 1; }
command -v jdeps >/dev/null 2>&1 || { echo 'jdeps is required' >&2; exit 1; }
command -v jar >/dev/null 2>&1 || { echo 'jar is required' >&2; exit 1; }

bash dev/fetch-plugin-api-libs.sh --check
mapfile -t jars < <(find plugin-api/lib -maxdepth 1 -type f -name '*.jar' -print | LC_ALL=C sort)
[ "${#jars[@]}" -eq 24 ] || {
  echo "plugin runtime must contain exactly 24 dependency jars; found ${#jars[@]}" >&2
  exit 1
}

scratch=$(mktemp -d "${TMPDIR:-/tmp}/foton-plugin-runtime.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
mkdir "$scratch/classes" "$scratch/extracted"
for jar in "${jars[@]}"; do
  absolute_jar="$(cd "$(dirname "$jar")" && pwd)/$(basename "$jar")"
  (cd "$scratch/extracted" && jar xf "$absolute_jar")
done
# Foton starts the JVM with -Djava.class.path, not --module-path. Strip module
# descriptors from this disposable view so jdeps evaluates that exact loading
# model; otherwise JOML's descriptor hides its Kotlin extension references
# behind JPMS readability errors even though they resolve on the classpath.
find "$scratch/extracted" -name module-info.class -delete
jdeps --multi-release base --missing-deps "$scratch/extracted" \
  > "$scratch/jdeps.log" 2>&1 || true
if grep -q -- '-> not found' "$scratch/jdeps.log"; then
  # Netty deliberately ships adapters for logging frameworks, BlockHound and
  # GraalVM native-image even when those optional tools are absent. Keep that
  # list narrow and visible; every other missing class still breaks the build.
  while read -r missing; do
    case "$missing" in
      com.oracle.svm.core.annotate.*|reactor.blockhound.*|\
      org.apache.commons.logging.*|org.apache.logging.log4j.*|org.apache.log4j.*) ;;
      *)
        cat "$scratch/jdeps.log" >&2
        echo "plugin runtime has an unresolved required class: $missing" >&2
        exit 1
        ;;
    esac
  done < <(awk 'NF >= 5 && $2 == "->" && $(NF - 1) == "not" && $NF == "found" {print $3}' \
    "$scratch/jdeps.log" | LC_ALL=C sort -u)
fi

javac -cp 'plugin-api/lib/*' -d "$scratch/classes" dev/PluginRuntimeCheck.java
cat > "$scratch/NettyRuntimeCheck.java" <<'JAVA'
import io.netty.buffer.ByteBuf;
import io.netty.buffer.Unpooled;
import io.netty.channel.ChannelHandlerContext;
import io.netty.channel.embedded.EmbeddedChannel;
import io.netty.handler.codec.ByteToMessageDecoder;
import io.netty.resolver.DefaultAddressResolverGroup;
import io.netty.util.ReferenceCountUtil;
import java.util.List;

public final class NettyRuntimeCheck {
    private NettyRuntimeCheck() { }

    public static void main(String[] args) {
        ByteToMessageDecoder decoder = new ByteToMessageDecoder() {
            @Override
            protected void decode(ChannelHandlerContext context, ByteBuf input, List<Object> output) {
                if (input.isReadable()) output.add(input.readRetainedSlice(input.readableBytes()));
            }
        };
        EmbeddedChannel channel = new EmbeddedChannel(decoder);
        if (!channel.writeInbound(Unpooled.wrappedBuffer(new byte[] {1, 2, 3}))) {
            throw new AssertionError("Netty transport did not publish the decoded buffer");
        }
        ByteBuf decoded = channel.readInbound();
        try {
            if (decoded.readableBytes() != 3 || decoded.readByte() != 1) {
                throw new AssertionError("Netty buffer/codec linkage changed data");
            }
            if (DefaultAddressResolverGroup.INSTANCE == null) {
                throw new AssertionError("Netty resolver was not linked");
            }
        } finally {
            ReferenceCountUtil.release(decoded);
            channel.finishAndReleaseAll();
        }
    }
}
JAVA
javac -cp 'plugin-api/lib/*' -d "$scratch/classes" "$scratch/NettyRuntimeCheck.java"
case "$(uname -s)" in
Linux|Darwin)
  runtime_classpath="$scratch/classes:plugin-api/lib/*"
  ;;
MINGW*|MSYS*|CYGWIN*)
  runtime_classpath="$(cygpath -w "$scratch/classes");$(cygpath -w "$PWD/plugin-api/lib")/*"
  ;;
*)
  echo 'unsupported Java classpath platform' >&2
  exit 1
  ;;
esac
java -cp "$runtime_classpath" PluginRuntimeCheck
java -cp "$runtime_classpath" NettyRuntimeCheck
printf 'plugin runtime dependency closure checked\n'
