package foton;

import org.bukkit.scoreboard.Scoreboard;
import org.bukkit.scoreboard.ScoreboardManager;

final class FotonScoreboardManager implements ScoreboardManager {
    @Override public Scoreboard getMainScoreboard() {
        String[] worlds = Native.worldNames();
        return new FotonScoreboard(worlds.length == 0 ? "" : worlds[0]);
    }
}
