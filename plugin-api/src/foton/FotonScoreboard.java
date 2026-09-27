package foton;

import java.util.Collections;
import java.util.LinkedHashSet;
import java.util.Set;
import org.bukkit.scoreboard.Scoreboard;
import org.bukkit.scoreboard.Team;

final class FotonScoreboard implements Scoreboard {
    private final String world;

    FotonScoreboard(String world) { this.world = world; }

    @Override
    public Team getEntryTeam(String entry) {
        if (entry == null) return null;
        String team = Native.scoreboardEntryTeam(world, entry);
        return team == null ? null : new FotonTeam(world, team);
    }

    @Override
    public Set<Team> getTeams() {
        String[] names = Native.scoreboardTeamNames(world);
        LinkedHashSet<Team> teams = new LinkedHashSet<>();
        if (names != null) {
            for (String name : names) {
                if (name != null) teams.add(new FotonTeam(world, name));
            }
        }
        return Collections.unmodifiableSet(teams);
    }

    @Override
    public Team getTeam(String name) {
        if (name == null) return null;
        String[] names = Native.scoreboardTeamNames(world);
        if (names == null) return null;
        for (String candidate : names) {
            if (name.equals(candidate)) return new FotonTeam(world, name);
        }
        return null;
    }

    @Override
    public Team registerNewTeam(String name) {
        if (name == null || name.isEmpty()) {
            throw new IllegalArgumentException("Team name cannot be null or empty");
        }
        if (!Native.scoreboardAddTeam(world, name)) {
            throw new IllegalArgumentException("Team already exists: " + name);
        }
        return new FotonTeam(world, name);
    }
}
