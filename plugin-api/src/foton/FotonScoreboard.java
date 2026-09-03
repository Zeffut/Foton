package foton;

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
}
