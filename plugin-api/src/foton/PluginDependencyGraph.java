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
        this(descriptors, Map.of(), Set.of());
    }

    PluginDependencyGraph(Iterable<PluginDescriptionFile> descriptors,
            Map<String, String> liveProviders, Set<String> liveNames) {
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
                // A live loser cannot acquire an alias on a later discovery pass.
                if (liveNames.contains(entry.getKey())
                        && !entry.getKey().equals(liveProviders.get(key(alias)))) continue;
                providers.putIfAbsent(key(alias), entry.getKey());
            }
        }
        providers.putAll(liveProviders);
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
                else if (edges.containsKey(hint.from()) && edges.containsKey(hint.to())) {
                    System.out.println("[host] recovered optional dependency cycle by skipping "
                        + plugins.get(hint.from()).getName() + " -> " + plugins.get(hint.to()).getName());
                }
            }
            List<String> ordered = sort(eligible, edges);
            if (ordered.size() == eligible.size()) return ordered;
            removeCycles(edges);
            ordered = sort(eligible, edges);
            if (ordered.size() != eligible.size()) {
                throw new IllegalStateException("unrecoverable " + phase + " plugin ordering cycle");
            }
            return ordered;
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

    Map<String, String> providers() { return Map.copyOf(providers); }
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

    /** Paper's Johnson recovery removes the closing edge of each discovered
     * cycle, then retries sorting. Work in Paper's consumer-to-dependency
     * direction; our ordinary sort stores the opposite direction. */
    private static void removeCycles(Map<String, Set<String>> edges) {
        Map<String, Set<String>> dependencies = new TreeMap<>();
        for (String node : edges.keySet()) dependencies.put(node, new TreeSet<>());
        for (var edge : edges.entrySet()) {
            for (String target : edge.getValue()) dependencies.get(target).add(edge.getKey());
        }
        Set<String> blocked = new HashSet<>();
        for (String start : dependencies.keySet()) {
            Map<String, Set<String>> induced = new TreeMap<>();
            for (String node : dependencies.keySet()) {
                if (node.compareTo(start) < 0) continue;
                Set<String> successors = new TreeSet<>(dependencies.get(node));
                successors.removeIf(next -> next.compareTo(start) < 0);
                induced.put(node, successors);
            }
            Set<String> component = new TreeSet<>();
            for (String node : induced.keySet()) {
                if (reachable(induced, start, node) && reachable(induced, node, start)) component.add(node);
            }
            if (component.size() < 2 && !dependencies.get(start).contains(start)) continue;
            Map<String, Set<String>> cycleGraph = new TreeMap<>();
            for (String node : component) {
                Set<String> successors = new TreeSet<>(dependencies.get(node));
                successors.retainAll(component);
                cycleGraph.put(node, successors);
            }
            blocked.removeAll(cycleGraph.get(start));
            removeClosingEdges(start, start, cycleGraph, blocked, dependencies);
        }
        for (Set<String> outgoing : edges.values()) outgoing.clear();
        for (var entry : dependencies.entrySet()) {
            for (String dependency : entry.getValue()) edges.get(dependency).add(entry.getKey());
        }
    }

    private static void removeClosingEdges(String start, String current,
            Map<String, Set<String>> cycleGraph, Set<String> blocked,
            Map<String, Set<String>> dependencies) {
        blocked.add(current);
        for (String successor : cycleGraph.get(current)) {
            if (successor.equals(start)) {
                dependencies.get(current).remove(successor);
                System.out.println("[host] recovered plugin ordering cycle by removing "
                    + current + " -> " + successor);
            } else if (!blocked.contains(successor)) {
                removeClosingEdges(start, successor, cycleGraph, blocked, dependencies);
            }
        }
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
