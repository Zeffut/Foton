package foton;

import java.util.Collections;
import java.util.LinkedHashSet;
import java.util.Set;
import org.bukkit.scoreboard.Scoreboard;
import org.bukkit.scoreboard.Team;

/** The scoreboard of the domain a world belongs to: the one {@code /team} changes there. */
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
    public Team getTeam(String name) {
        if (name == null) throw new IllegalArgumentException("team name cannot be null");
        return Native.scoreboardTeamProperty(world, name, "color") == null ? null : new FotonTeam(world, name);
    }

    @Override
    public Set<Team> getTeams() {
        Set<Team> teams = new LinkedHashSet<>();
        String[] names = Native.scoreboardTeamNames(world);
        if (names != null) for (String name : names) teams.add(new FotonTeam(world, name));
        return Collections.unmodifiableSet(teams);
    }

    @Override
    public Team registerNewTeam(String name) {
        if (name == null || name.isEmpty()) throw new IllegalArgumentException("team name cannot be empty");
        if (!Native.scoreboardRegisterTeam(world, name)) {
            throw new IllegalArgumentException("Team name '" + name + "' is already in use");
        }
        return new FotonTeam(world, name);
    }

    @Override
    public boolean equals(Object other) {
        return other instanceof FotonScoreboard scoreboard && world.equals(scoreboard.world);
    }

    @Override
    public int hashCode() {
        return world.hashCode();
    }
}
