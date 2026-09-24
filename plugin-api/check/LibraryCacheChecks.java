package foton;

import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.net.URI;
import java.net.URL;
import java.net.URLConnection;
import java.net.URLStreamHandler;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Arrays;
import java.util.Comparator;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.Executors;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.jar.JarEntry;
import java.util.jar.JarOutputStream;

/** Regression checks for crash-safe runtime library caching. */
public final class LibraryCacheChecks {
    private LibraryCacheChecks() {}

    public static void check() throws Exception {
        Path work = Files.createTempDirectory("foton-library-cache-check.");
        try {
            byte[] complete = libraryBytes();
            failedDownloadDoesNotPublish(work, complete);
            partialDownloadDoesNotPublish(work, complete);
            corruptCacheIsReplaced(work, complete);
            validCacheAvoidsNetwork(work, complete);
            concurrentDownloadsPublishOneCompleteJar(work, complete);
        } finally {
            deleteTree(work);
        }
    }

    private static void partialDownloadDoesNotPublish(Path work, byte[] complete)
            throws Exception {
        Path target = work.resolve("partial/library.jar");
        byte[] partial = Arrays.copyOf(complete, complete.length / 2);

        boolean failed = false;
        try {
            PluginHost.cacheLibrary(source(partial, -1, null, null, new AtomicInteger()),
                target);
        } catch (IOException expected) {
            failed = true;
        }
        expect(failed, "a clean EOF in the middle of a jar must fail validation");
        expect(!Files.exists(target), "a partial jar must never become the cache entry");
        expect(!hasPartialSibling(target),
            "a rejected partial jar must remove its private staging file");
    }

    private static void failedDownloadDoesNotPublish(Path work, byte[] complete)
            throws Exception {
        Path target = work.resolve("cut/library.jar");
        Files.createDirectories(target.getParent());
        byte[] corrupt = new byte[] {0x46, 0x4f, 0x54, 0x4f, 0x4e};
        Files.write(target, corrupt);

        boolean failed = false;
        try {
            PluginHost.cacheLibrary(source(complete, complete.length / 2, null, null,
                new AtomicInteger()), target);
        } catch (IOException expected) {
            failed = true;
        }
        expect(failed, "a cut-off library download must fail");
        expect(Arrays.equals(corrupt, Files.readAllBytes(target)),
            "a failed download must preserve the previous cache entry");
        expect(!hasPartialSibling(target),
            "a failed download must remove its private partial file");
    }

    private static void corruptCacheIsReplaced(Path work, byte[] complete) throws Exception {
        Path target = work.resolve("corrupt/library.jar");
        Files.createDirectories(target.getParent());
        try (JarOutputStream ignored = new JarOutputStream(Files.newOutputStream(target))) {
            // A syntactically valid but empty jar is not a usable runtime library.
        }

        PluginHost.cacheLibrary(source(complete, -1, null, null, new AtomicInteger()),
            target);
        expect(Arrays.equals(complete, Files.readAllBytes(target)),
            "a corrupt cache entry must be replaced by the complete jar");
        expect(!hasPartialSibling(target),
            "a successful download must remove its private staging name");
    }

    private static void validCacheAvoidsNetwork(Path work, byte[] complete) throws Exception {
        Path target = work.resolve("valid/library.jar");
        Files.createDirectories(target.getParent());
        Files.write(target, complete);
        AtomicInteger opens = new AtomicInteger();

        PluginHost.cacheLibrary(source(new byte[0], 0, null, null, opens), target);
        expect(opens.get() == 0, "a valid cache entry must not reopen the network source");
        expect(Arrays.equals(complete, Files.readAllBytes(target)),
            "using a valid cache entry must not rewrite it");
    }

    private static void concurrentDownloadsPublishOneCompleteJar(Path work, byte[] complete)
            throws Exception {
        Path target = work.resolve("concurrent/library.jar");
        int callers = 6;
        CountDownLatch ready = new CountDownLatch(callers);
        CountDownLatch start = new CountDownLatch(1);
        CountDownLatch reading = new CountDownLatch(1);
        CountDownLatch release = new CountDownLatch(1);
        URL source = source(complete, -1, reading, release, new AtomicInteger());
        var pool = Executors.newFixedThreadPool(callers);
        try {
            var futures = new java.util.ArrayList<java.util.concurrent.Future<?>>();
            for (int index = 0; index < callers; index++) {
                futures.add(pool.submit(() -> {
                    ready.countDown();
                    await(start);
                    PluginHost.cacheLibrary(source, target);
                    return null;
                }));
            }
            expect(ready.await(5, TimeUnit.SECONDS), "concurrent cache callers did not start");
            start.countDown();
            expect(reading.await(5, TimeUnit.SECONDS), "a concurrent download did not begin");
            release.countDown();
            for (var future : futures) future.get(5, TimeUnit.SECONDS);
        } finally {
            release.countDown();
            pool.shutdownNow();
            pool.awaitTermination(5, TimeUnit.SECONDS);
        }

        expect(Arrays.equals(complete, Files.readAllBytes(target)),
            "concurrent downloads must publish a complete jar");
        expect(!hasPartialSibling(target),
            "concurrent downloads must clean every private partial file");
    }

    private static byte[] libraryBytes() throws IOException {
        ByteArrayOutputStream bytes = new ByteArrayOutputStream();
        try (JarOutputStream jar = new JarOutputStream(bytes)) {
            jar.putNextEntry(new JarEntry("example/resource.txt"));
            jar.write("complete runtime library".getBytes(java.nio.charset.StandardCharsets.UTF_8));
            jar.closeEntry();
        }
        return bytes.toByteArray();
    }

    private static URL source(byte[] bytes, int failAfter, CountDownLatch reading,
            CountDownLatch release, AtomicInteger opens) throws Exception {
        return URL.of(URI.create("memory://foton/library.jar"), new URLStreamHandler() {
            @Override protected URLConnection openConnection(URL url) {
                opens.incrementAndGet();
                return new URLConnection(url) {
                    @Override public void connect() {}

                    @Override public InputStream getInputStream() {
                        if (reading != null) reading.countDown();
                        return new InterruptingInputStream(bytes, failAfter, release);
                    }
                };
            }
        });
    }

    private static boolean hasPartialSibling(Path target) throws IOException {
        String prefix = "." + target.getFileName() + ".";
        try (var children = Files.list(target.getParent())) {
            return children.anyMatch(path -> {
                String name = path.getFileName().toString();
                return name.startsWith(prefix) && name.endsWith(".part");
            });
        }
    }

    private static void await(CountDownLatch latch) throws IOException {
        try {
            latch.await();
        } catch (InterruptedException error) {
            Thread.currentThread().interrupt();
            throw new IOException("interrupted while reading the test library", error);
        }
    }

    private static void deleteTree(Path root) throws IOException {
        try (var paths = Files.walk(root)) {
            for (Path path : paths.sorted(Comparator.reverseOrder()).toList()) {
                Files.deleteIfExists(path);
            }
        }
    }

    private static void expect(boolean condition, String message) {
        if (!condition) throw new AssertionError(message);
    }

    private static final class InterruptingInputStream extends InputStream {
        private final ByteArrayInputStream input;
        private final int failAfter;
        private final CountDownLatch release;
        private int consumed;

        private InterruptingInputStream(byte[] bytes, int failAfter, CountDownLatch release) {
            input = new ByteArrayInputStream(bytes);
            this.failAfter = failAfter;
            this.release = release;
        }

        @Override public int read() throws IOException {
            if (consumed == 0 && release != null) await(release);
            if (failAfter >= 0 && consumed >= failAfter) {
                throw new IOException("simulated connection loss");
            }
            int value = input.read();
            if (value >= 0) consumed++;
            return value;
        }

        @Override public int read(byte[] output, int offset, int length) throws IOException {
            if (length == 0) return 0;
            int first = read();
            if (first < 0) return -1;
            output[offset] = (byte) first;
            int count = 1;
            while (count < length && input.available() > 0
                    && (failAfter < 0 || consumed < failAfter)) {
                output[offset + count] = (byte) input.read();
                count++;
                consumed++;
            }
            return count;
        }
    }
}
