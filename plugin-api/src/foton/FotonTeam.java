package foton;

import java.util.Arrays;
import java.util.Collections;
import java.util.LinkedHashSet;
import java.util.Set;
import org.bukkit.scoreboard.Team;

final class FotonTeam implements Team {
    private final String world;
    private final String name;

    FotonTeam(String world, String name) { this.world = world; this.name = name; }

    @Override public String getName() { return name; }

    @Override
    public Set<String> getEntries() {
        requireRegistered();
        String[] entries = Native.scoreboardTeamEntries(world, name);
        if (entries == null) return Collections.emptySet();
        return Collections.unmodifiableSet(new LinkedHashSet<>(Arrays.asList(entries)));
    }

    @Override
    public void addEntry(String entry) {
        requireEntry(entry);
        requireRegistered();
        if (!name.equals(Native.scoreboardEntryTeam(world, entry))) {
            Native.scoreboardAddEntry(world, name, entry);
        }
    }

    @Override
    public boolean removeEntry(String entry) {
        requireEntry(entry);
        requireRegistered();
        return Native.scoreboardRemoveEntry(world, name, entry);
    }

    @Override
    public boolean hasEntry(String entry) {
        if (entry == null) return false;
        requireRegistered();
        return name.equals(Native.scoreboardEntryTeam(world, entry));
    }

    @Override
    public net.kyori.adventure.text.Component prefix() {
        requireRegistered();
        String encoded = Native.scoreboardTeamPrefix(world, name);
        if (encoded == null) {
            throw new IllegalStateException("Team prefix is unavailable: " + name);
        }
        return net.kyori.adventure.text.serializer.gson.GsonComponentSerializer.gson()
            .deserialize(encoded);
    }

    @Override
    public void prefix(net.kyori.adventure.text.Component prefix) {
        if (prefix == null) throw new IllegalArgumentException("prefix");
        requireRegistered();
        String encoded = net.kyori.adventure.text.serializer.gson.GsonComponentSerializer.gson()
            .serialize(prefix);
        if (!Native.scoreboardSetTeamPrefix(world, name, encoded)) {
            throw new IllegalStateException("Team is no longer registered: " + name);
        }
    }

    @Override
    public void unregister() {
        requireRegistered();
        if (!Native.scoreboardRemoveTeam(world, name)) {
            throw new IllegalStateException("Team is no longer registered: " + name);
        }
    }

    private void requireRegistered() {
        String[] names = Native.scoreboardTeamNames(world);
        if (names != null) {
            for (String candidate : names) {
                if (name.equals(candidate)) return;
            }
        }
        throw new IllegalStateException("Team is no longer registered: " + name);
    }

    private static void requireEntry(String entry) {
        if (entry == null || entry.isEmpty()) {
            throw new IllegalArgumentException("Entry cannot be null or empty");
        }
    }

    @Override
    public boolean equals(Object other) {
        return other instanceof FotonTeam team
            && world.equals(team.world) && name.equals(team.name);
    }

    @Override
    public int hashCode() {
        return 31 * world.hashCode() + name.hashCode();
    }
}
