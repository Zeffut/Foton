package org.bukkit.boss;

import java.util.List;
import org.bukkit.entity.Player;

public interface BossBar {
    String getTitle();
    void setTitle(String title);
    BarColor getColor();
    void setColor(BarColor color);
    BarStyle getStyle();
    void setStyle(BarStyle style);
    void removeFlag(BarFlag flag);
    void addFlag(BarFlag flag);
    boolean hasFlag(BarFlag flag);
    void setProgress(double progress);
    double getProgress();
    void addPlayer(Player player);
    void removePlayer(Player player);
    void removeAll();
    List<Player> getPlayers();
    void setVisible(boolean visible);
    boolean isVisible();

    @Deprecated(since = "1.9")
    void show();

    @Deprecated(since = "1.9")
    void hide();
}
