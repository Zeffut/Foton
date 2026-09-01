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
        String[] entries = Native.scoreboardTeamEntries(world, name);
        return Collections.unmodifiableSet(new LinkedHashSet<>(Arrays.asList(entries)));
    }
}
