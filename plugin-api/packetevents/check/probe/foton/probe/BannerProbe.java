package foton.probe;

import java.util.List;
import java.util.Map;
import org.bukkit.DyeColor;
import org.bukkit.Location;
import org.bukkit.Material;
import org.bukkit.block.Banner;
import org.bukkit.block.Block;
import org.bukkit.block.BlockState;
import org.bukkit.block.banner.Pattern;
import org.bukkit.block.banner.PatternType;
import org.bukkit.command.Command;
import org.bukkit.command.CommandExecutor;
import org.bukkit.command.CommandSender;
import org.bukkit.entity.Player;

/** `/probebanner`: puts a banner two blocks from the player and walks the
 * block state's pattern API the way Zelda Civ's shield workshop does, writing
 * down what a snapshot says before and after `update`. */
final class BannerProbe implements CommandExecutor {
    private final Map<String, String> facts;

    BannerProbe(Map<String, String> facts) {
        this.facts = facts;
    }

    @Override
    public boolean onCommand(CommandSender sender, Command command, String label, String[] args) {
        if (!(sender instanceof Player player)) return true;
        try {
            run(player.getLocation().add(2, 0, 0).getBlock());
        } catch (Throwable error) {
            facts.put("banner", "threw " + error);
        }
        return true;
    }

    private static String describe(List<Pattern> patterns) {
        StringBuilder text = new StringBuilder("[");
        for (Pattern pattern : patterns) {
            if (text.length() > 1) text.append(", ");
            text.append(pattern.getPattern().getKey()).append(':').append(pattern.getColor());
        }
        return text.append(']').toString();
    }

    private void run(Block block) {
        // Banners break when nothing holds them, so give each something to hold.
        block.getRelative(0, -1, 0).setType(Material.STONE);
        block.getRelative(-1, 0, 0).setType(Material.STONE);
        block.setType(Material.WHITE_BANNER);
        Banner banner = (Banner) block.getState();
        facts.put("banner fresh", banner.getBaseColor() + " " + describe(banner.getPatterns()));

        banner.setPatterns(List.of(new Pattern(DyeColor.RED, PatternType.CROSS),
            new Pattern(DyeColor.BLACK, PatternType.BORDER)));
        banner.addPattern(new Pattern(DyeColor.BLUE, PatternType.STRIPE_TOP));
        facts.put("banner edited snapshot", banner.numberOfPatterns() + " " + describe(banner.getPatterns()));
        facts.put("banner block before update", describe(((Banner) block.getState()).getPatterns()));

        banner.removePattern(0);
        banner.setPattern(0, new Pattern(DyeColor.LIME, PatternType.BORDER));
        facts.put("banner getPattern(1)", String.valueOf(banner.getPattern(1).getPattern().getKey()));
        boolean updated = banner.update();
        facts.put("banner update", String.valueOf(updated));
        facts.put("banner block after update", describe(((Banner) block.getState()).getPatterns()));

        Banner untouched = (Banner) block.getState();
        untouched.update();
        facts.put("banner untouched update", describe(((Banner) block.getState()).getPatterns()));

        Banner recolor = (Banner) block.getState();
        recolor.setBaseColor(DyeColor.RED);
        recolor.update();
        BlockState after = block.getState();
        facts.put("banner recolored", block.getType() + " "
            + ((Banner) after).getBaseColor() + " " + describe(((Banner) after).getPatterns()));

        Banner cleared = (Banner) block.getState();
        cleared.setPatterns(List.of());
        cleared.update();
        facts.put("banner cleared", describe(((Banner) block.getState()).getPatterns()));

        // A wall banner keeps its facing and its layers when it is recolored.
        block.setBlockData(org.bukkit.Bukkit.createBlockData("minecraft:white_wall_banner[facing=east]"));
        Banner wall = (Banner) block.getState();
        wall.addPattern(new Pattern(DyeColor.GREEN, PatternType.CROSS));
        wall.setBaseColor(DyeColor.BLUE);
        wall.update();
        facts.put("banner wall recolored", block.getBlockData().getAsString() + " "
            + describe(((Banner) block.getState()).getPatterns()));
        block.setType(Material.AIR);
    }
}
