import java.io.BufferedReader;
import java.io.ByteArrayInputStream;
import java.io.StringReader;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;

/** Direct probes of the real pinned server classes, or Foton, without a world boot. */
public final class PaperGraphParity {
    public static void main(String[] args) throws Exception {
        boolean paper = args[0].equals("paper");
        for (String version : List.of("garbage", "1.18", "none", "1", "1.2.3.4", "1.x",
                "1.19", "26.2", "99.99", "+26.02", "26.2.")) {
            try {
                Object descriptor = descriptor(paper, "Version", version, "");
                var getter = descriptor.getClass().getMethod("getAPIVersion");
                getter.setAccessible(true);
                System.out.println(version + "=" + getter.invoke(descriptor));
            } catch (java.lang.reflect.InvocationTargetException error) {
                String type = error.getCause().getClass().getName();
                if (!type.equals("org.bukkit.plugin.InvalidDescriptionException")
                        && !type.startsWith("org.spongepowered.configurate.")) throw error;
                System.out.println(version + "=rejected");
            }
        }
        if (paper) {
            Class<?> graphType = Class.forName("com.google.common.graph.MutableGraph");
            Class<?> builderType = Class.forName("com.google.common.graph.GraphBuilder");
            Object graph = builderType.getMethod("build").invoke(builderType.getMethod("directed").invoke(null));
            var edge = graphType.getMethod("putEdge", Object.class, Object.class);
            edge.invoke(graph, "OptionalA", "OptionalB");
            edge.invoke(graph, "OptionalB", "OptionalA");
            edge.invoke(graph, "Downstream", "OptionalA");
            Class<?> tree = Class.forName("io.papermc.paper.plugin.entrypoint.strategy.modern.LoadOrderTree");
            var constructor = tree.getDeclaredConstructor(java.util.Map.class, graphType);
            constructor.setAccessible(true);
            var order = tree.getDeclaredMethod("getLoadOrder");
            order.setAccessible(true);
            System.out.println(order.invoke(constructor.newInstance(java.util.Map.of(), graph)).toString().toLowerCase(java.util.Locale.ROOT));
        } else {
            List<Object> descriptors = new ArrayList<>();
            descriptors.add(descriptor(false, "OptionalA", "26.2", dependency("OptionalB")));
            descriptors.add(descriptor(false, "OptionalB", "26.2", dependency("OptionalA")));
            descriptors.add(descriptor(false, "Downstream", "26.2", dependency("OptionalA")));
            Class<?> graph = Class.forName("foton.PluginDependencyGraph");
            var constructor = graph.getDeclaredConstructor(Iterable.class);
            constructor.setAccessible(true);
            Class<?> phase = Class.forName("foton.PaperPluginDescriptor$Phase");
            var order = graph.getDeclaredMethod("order", phase);
            order.setAccessible(true);
            System.out.println(order.invoke(constructor.newInstance(descriptors), phase.getEnumConstants()[1]));
        }
    }

    private static String dependency(String name) {
        return "dependencies:\n  server:\n    " + name + ":\n      load: BEFORE\n      required: false\n";
    }

    private static Object descriptor(boolean paper, String name, String version, String fields) throws Exception {
        String yaml = "name: " + name + "\nversion: 1\nmain: fixture.Main\napi-version: '" + version + "'\n" + fields;
        if (paper) {
            return Class.forName("io.papermc.paper.plugin.provider.configuration.PaperPluginMeta")
                .getMethod("create", BufferedReader.class).invoke(null, new BufferedReader(new StringReader(yaml)));
        }
        var read = Class.forName("foton.PaperPluginDescriptor").getDeclaredMethod("read", java.io.InputStream.class);
        read.setAccessible(true);
        return read.invoke(null, new ByteArrayInputStream(yaml.getBytes(StandardCharsets.UTF_8)));
    }
}
