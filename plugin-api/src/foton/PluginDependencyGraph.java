package foton;

import java.util.ArrayList;
import java.util.Comparator;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Set;
import java.util.TreeMap;
import java.util.TreeSet;
import org.bukkit.plugin.PluginDescriptionFile;
import org.bukkit.plugin.PluginLoadOrder;

/** Resolves presence, ordering and visibility independently for each Paper phase. */
final class PluginDependencyGraph {
    private final Map<String, PluginDescriptionFile> plugins = new TreeMap<>();
    private final Map<String, String> providers = new HashMap<>();

    PluginDependencyGraph(Iterable<PluginDescriptionFile> descriptors) {
        for (PluginDescriptionFile descriptor : descriptors) {
            String key = key(descriptor.getName());
            if (plugins.putIfAbsent(key, descriptor) != null) {
                throw new IllegalArgumentException("duplicate plugin name " + descriptor.getName());
            }
            providers.put(key, key);
        }
        // A real plugin name has priority over a provided alias.
        for (var entry : plugins.entrySet()) {
            for (String alias : entry.getValue().getProvides()) {
                providers.putIfAbsent(key(alias), entry.getKey());
            }
        }
    }

    List<String> order(PaperPluginDescriptor.Phase phase) {
        Set<String> eligible = new TreeSet<>(plugins.keySet());
        if (phase == PaperPluginDescriptor.Phase.BOOTSTRAP) {
            eligible.removeIf(key -> !(plugins.get(key) instanceof PaperPluginDescriptor paper)
                || paper.bootstrapper() == null);
        }
        for (;;) {
            boolean removed = eligible.removeIf(key -> missingRequired(key, phase, eligible));
            if (removed) continue;
            Map<String, Set<String>> edges = new TreeMap<>();
            for (String key : eligible) edges.put(key, new TreeSet<>());
            List<Edge> hints = new ArrayList<>();
            for (String key : eligible) {
                PluginDescriptionFile descriptor = plugins.get(key);
                if (descriptor instanceof PaperPluginDescriptor paper) {
                    for (var dependency : paper.paperDependencies()) {
                        if (dependency.phase() != phase) continue;
                        String target = provider(dependency.name());
                        if (dependency.load() == PaperPluginDescriptor.LoadOrder.BEFORE) add(edges, target, key);
                        if (dependency.load() == PaperPluginDescriptor.LoadOrder.AFTER) add(edges, key, target);
                    }
                } else {
                    for (String name : descriptor.getDepend()) add(edges, provider(name), key);
                    for (String name : descriptor.getSoftDepend()) hints.add(new Edge(provider(name), key));
                    for (String name : descriptor.getLoadBefore()) hints.add(new Edge(key, provider(name)));
                }
            }
            hints.sort(Comparator.comparing(Edge::from).thenComparing(Edge::to));
            for (Edge hint : hints) {
                if (!reachable(edges, hint.to(), hint.from())) add(edges, hint.from(), hint.to());
            }
            List<String> ordered = sort(eligible, edges);
            if (ordered.size() == eligible.size()) return ordered;
            Set<String> resolved = new HashSet<>(ordered);
            for (String key : eligible) {
                if (!resolved.contains(key)) System.out.println("[host] " + phase
                    + " dependency cycle or dependent involving " + plugins.get(key).getName());
            }
            eligible.retainAll(resolved);
            // Required OMIT/AFTER edges do not impose dependency-first order, but
            // their consumers must still be removed when the provider is rejected.
        }
    }

    private boolean missingRequired(String key, PaperPluginDescriptor.Phase phase, Set<String> eligible) {
        for (String name : required(plugins.get(key), phase)) {
            if (eligible.contains(provider(name))) continue;
            System.out.println("[host] " + plugins.get(key).getName()
                + ": missing or unavailable " + phase + " dependency " + name);
            return true;
        }
        return false;
    }

    static List<String> required(PluginDescriptionFile descriptor, PaperPluginDescriptor.Phase phase) {
        if (!(descriptor instanceof PaperPluginDescriptor paper)) {
            return phase == PaperPluginDescriptor.Phase.SERVER ? descriptor.getDepend() : List.of();
        }
        return paper.paperDependencies().stream().filter(d -> d.phase() == phase && d.required())
            .map(PaperPluginDescriptor.Dependency::name).toList();
    }

    static List<String> visible(PluginDescriptionFile descriptor, PaperPluginDescriptor.Phase phase) {
        if (descriptor instanceof PaperPluginDescriptor paper) {
            return paper.paperDependencies().stream().filter(d -> d.phase() == phase && d.joinClasspath())
                .map(PaperPluginDescriptor.Dependency::name).toList();
        }
        if (phase != PaperPluginDescriptor.Phase.SERVER) return List.of();
        List<String> names = new ArrayList<>(descriptor.getDepend());
        names.addAll(descriptor.getSoftDepend());
        return List.copyOf(names);
    }

    private String provider(String name) { return providers.getOrDefault(key(name), key(name)); }
    private static String key(String name) { return name.toLowerCase(Locale.ROOT); }

    private static void add(Map<String, Set<String>> edges, String from, String to) {
        if (edges.containsKey(from) && edges.containsKey(to)) edges.get(from).add(to);
    }

    private static boolean reachable(Map<String, Set<String>> edges, String start, String target) {
        Set<String> seen = new HashSet<>();
        var pending = new java.util.ArrayDeque<String>();
        pending.add(start);
        while (!pending.isEmpty()) {
            String current = pending.removeFirst();
            if (current.equals(target)) return true;
            if (seen.add(current)) pending.addAll(edges.getOrDefault(current, Set.of()));
        }
        return false;
    }

    private List<String> sort(Set<String> nodes, Map<String, Set<String>> edges) {
        Map<String, Integer> indegree = new HashMap<>();
        for (String key : nodes) indegree.put(key, 0);
        for (var outgoing : edges.values()) for (String target : outgoing) indegree.merge(target, 1, Integer::sum);
        var ready = new java.util.PriorityQueue<String>(Comparator
            .comparing((String key) -> plugins.get(key).getLoad() != PluginLoadOrder.STARTUP)
            .thenComparing(Comparator.naturalOrder()));
        for (var entry : indegree.entrySet()) if (entry.getValue() == 0) ready.add(entry.getKey());
        List<String> ordered = new ArrayList<>();
        while (!ready.isEmpty()) {
            String key = ready.remove();
            ordered.add(key);
            for (String target : edges.get(key)) if (indegree.merge(target, -1, Integer::sum) == 0) ready.add(target);
        }
        return ordered;
    }

    private record Edge(String from, String to) {}
}
