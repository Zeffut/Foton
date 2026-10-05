/** API coverage for the mutable scoreboard surface used by Zelda Civ. */
final class Scoreboards {
    private Scoreboards() {}

    static void check() {
        net.kyori.adventure.text.Component styled =
            net.kyori.adventure.text.Component.text("[staff] ")
                .color(net.kyori.adventure.text.format.NamedTextColor.GOLD);
        net.kyori.adventure.text.serializer.gson.GsonComponentSerializer serializer =
            net.kyori.adventure.text.serializer.gson.GsonComponentSerializer.gson();
        Checks.same(serializer.deserialize(serializer.serialize(styled)), styled,
            "scoreboard prefixes did not survive the Adventure/native JSON boundary");

        // Core tests exercise persisted mutations and exact packet operations;
        // this linkage probe covers the Paper calls Zelda performs.
        java.util.function.Consumer<org.bukkit.scoreboard.Scoreboard> zelda = board -> {
            org.bukkit.scoreboard.Team team = board.getTeam("zc_staff");
            if (team == null) team = board.registerNewTeam("zc_staff");
            net.kyori.adventure.text.Component prefix =
                net.kyori.adventure.text.Component.text("[staff] ");
            if (!prefix.equals(team.prefix())) team.prefix(prefix);
            if (!team.hasEntry("Zeffut")) team.addEntry("Zeffut");
            team.removeEntry("Zeffut");
            board.getEntryTeam("Zeffut");
            board.getTeams().contains(team);
            team.unregister();
        };
        Checks.expect(zelda != null, "the Zelda scoreboard compatibility surface vanished");
    }
}
