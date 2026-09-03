package foton;

import org.bukkit.scoreboard.Scoreboard;
import org.bukkit.scoreboard.ScoreboardManager;

final class FotonScoreboardManager implements ScoreboardManager {
    private Scoreboard main;

    @Override public Scoreboard getMainScoreboard() {
        if (main == null) {
            String[] worlds = Native.worldNames();
            main = new FotonScoreboard(worlds == null || worlds.length == 0 ? "" : worlds[0]);
        }
        return main;
    }
}
