import org.bukkit.ChatColor;
import org.bukkit.block.BlockFace;

/** Color codes and block faces: small, and wrong in ways that are quiet. */
final class Colors {
    private Colors() {}

    static void check() {
        // Plugins concatenate these into strings, so toString is the code.
        Checks.same(ChatColor.RED.toString(), "\u00A7c", "a color is its code");
        Checks.same(ChatColor.RESET.getChar(), 'r', "reset is r");

        // A config file holds `&a`, and this is what turns it into a color.
        Checks.same(ChatColor.translateAlternateColorCodes('&', "&ahello &lworld"),
            "\u00A7ahello \u00A7lworld", "alternate codes translate");
        Checks.same(ChatColor.translateAlternateColorCodes('&', "100% & rising"),
            "100% & rising", "an ampersand that is not a code stays one");
        Checks.same(ChatColor.translateAlternateColorCodes('&', "trailing &"), "trailing &",
            "an ampersand at the end is not a code");

        Checks.same(ChatColor.stripColor("\u00A7ahello"), "hello", "stripping a color");
        Checks.same(ChatColor.stripColor(null), null, "stripping nothing");

        // The offsets are vanilla's, so getRelative lands where a plugin
        // written against Bukkit expects.
        Checks.same(BlockFace.NORTH.getModZ(), -1, "north is negative z");
        Checks.same(BlockFace.UP.getModY(), 1, "up is positive y");
        Checks.same(BlockFace.NORTH.getOppositeFace(), BlockFace.SOUTH, "north faces south");
        Checks.same(BlockFace.SELF.getOppositeFace(), BlockFace.SELF, "self faces itself");

        relative();
    }

    /** A block's neighbour is the block it should be. */
    private static void relative() {
        foton.FotonBlock block = new foton.FotonBlock(null, 10, 64, -5);

        Checks.same(block.getRelative(BlockFace.UP).getY(), 65, "up is one higher");
        Checks.same(block.getRelative(BlockFace.NORTH).getZ(), -6, "north is one lower in z");
        Checks.same(block.getRelative(BlockFace.EAST, 3).getX(), 13, "three east");
        Checks.same(block.getRelative(2, 0, 0).getX(), 12, "two by coordinates");

        // The original does not move: a plugin walking a line calls this in a
        // loop and would otherwise walk away from where it started.
        Checks.same(block.getX(), 10, "getRelative does not move the block it was asked");
    }
}
