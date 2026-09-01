package foton;

import java.lang.ref.Cleaner;
import java.util.ArrayList;
import java.util.EnumSet;
import java.util.List;
import java.util.Objects;
import java.util.UUID;
import org.bukkit.boss.BarColor;
import org.bukkit.boss.BarFlag;
import org.bukkit.boss.BarStyle;
import org.bukkit.boss.BossBar;
import org.bukkit.entity.Player;

/** A Bukkit view of Foton's native {@code ServerBossEvent}. */
@SuppressWarnings("deprecation")
final class FotonBossBar implements BossBar {
    private static final Cleaner CLEANER = Cleaner.create();

    private final String id;
    private final Cleaner.Cleanable cleanable;
    private final EnumSet<BarFlag> flags = EnumSet.noneOf(BarFlag.class);
    private String title;
    private BarColor color;
    private BarStyle style;
    private double progress = 1.0;
    private boolean visible = true;

    FotonBossBar(String title, BarColor color, BarStyle style, BarFlag... flags) {
        this.title = title == null ? "" : title;
        this.color = Objects.requireNonNull(color, "color");
        this.style = Objects.requireNonNull(style, "style");
        if (flags != null) {
            for (BarFlag flag : flags) {
                this.flags.add(Objects.requireNonNull(flag, "flag"));
            }
        }
        id = Native.createBossBar(this.title, color.ordinal(), style.ordinal(), flagMask());
        cleanable = CLEANER.register(this, new Release(id));
    }

    @Override public String getTitle() {
        return title;
    }

    @Override public void setTitle(String title) {
        this.title = title == null ? "" : title;
        Native.bossBarSetTitle(id, this.title);
    }

    @Override public BarColor getColor() {
        return color;
    }

    @Override public void setColor(BarColor color) {
        this.color = Objects.requireNonNull(color, "color");
        Native.bossBarSetColor(id, color.ordinal());
    }

    @Override public BarStyle getStyle() {
        return style;
    }

    @Override public void setStyle(BarStyle style) {
        this.style = Objects.requireNonNull(style, "style");
        Native.bossBarSetStyle(id, style.ordinal());
    }

    @Override public void removeFlag(BarFlag flag) {
        if (flags.remove(Objects.requireNonNull(flag, "flag"))) {
            Native.bossBarSetFlags(id, flagMask());
        }
    }

    @Override public void addFlag(BarFlag flag) {
        if (flags.add(Objects.requireNonNull(flag, "flag"))) {
            Native.bossBarSetFlags(id, flagMask());
        }
    }

    @Override public boolean hasFlag(BarFlag flag) {
        return flags.contains(Objects.requireNonNull(flag, "flag"));
    }

    @Override public void setProgress(double progress) {
        if (!(progress >= 0.0 && progress <= 1.0)) {
            throw new IllegalArgumentException("progress must be between 0.0 and 1.0");
        }
        this.progress = progress;
        Native.bossBarSetProgress(id, progress);
    }

    @Override public double getProgress() {
        return progress;
    }

    @Override public void addPlayer(Player player) {
        Native.bossBarAddPlayer(id,
            Objects.requireNonNull(player, "player").getUniqueId().toString());
    }

    @Override public void removePlayer(Player player) {
        Native.bossBarRemovePlayer(id,
            Objects.requireNonNull(player, "player").getUniqueId().toString());
    }

    @Override public void removeAll() {
        Native.bossBarRemoveAll(id);
    }

    @Override public List<Player> getPlayers() {
        List<Player> players = new ArrayList<>();
        for (String value : Native.bossBarPlayerIds(id)) {
            UUID uuid = Native.parse(value);
            Player player = uuid == null ? null : org.bukkit.Bukkit.getPlayer(uuid);
            if (player != null) {
                players.add(player);
            }
        }
        return List.copyOf(players);
    }

    @Override public void setVisible(boolean visible) {
        this.visible = visible;
        Native.bossBarSetVisible(id, visible);
    }

    @Override public boolean isVisible() {
        return visible;
    }

    @Override public void show() {
        setVisible(true);
    }

    @Override public void hide() {
        setVisible(false);
    }

    private int flagMask() {
        int mask = 0;
        for (BarFlag flag : flags) {
            mask |= 1 << flag.ordinal();
        }
        return mask;
    }

    private record Release(String id) implements Runnable {
        @Override public void run() {
            Native.releaseBossBar(id);
        }
    }
}
