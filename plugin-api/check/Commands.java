import java.util.ArrayList;
import java.util.List;
import org.bukkit.command.CommandSender;

/** The command path, from a typed line to a plugin's onCommand. */
final class Commands {
    private Commands() {}

    static void check() {
        Recorder player = new Recorder("Ada");

        // The line arrives exactly as typed, without its slash. A name no
        // plugin claimed has to answer false, because Foton reads that as
        // permission to go on to its own dispatcher -- answering true would
        // swallow every vanilla command on the server.
        Checks.expect(!foton.CommandMap.dispatch(player, "gamemode creative"),
            "an unclaimed name must not be owned");
        Checks.expect(!foton.CommandMap.dispatch(player, ""), "an empty line is nobody's");

        Checks.expect(foton.CommandMap.dispatch(player, "fixture hello world"),
            "a declared command should be owned");
        Checks.same(example.EventFixture.commandLabel, "fixture", "the label as typed");
        Checks.same(List.of(example.EventFixture.commandArgs), List.of("hello", "world"),
            "the arguments after the name");
        Checks.same(example.EventFixture.commandSender, "Ada", "who ran it");
        Checks.same(player.last(), "fixture ran with hello world", "what the handler sent back");

        // Bukkit splits on runs of spaces, so double-tapping the space bar
        // does not send an empty argument.
        foton.CommandMap.dispatch(player, "fixture  spaced   out ");
        Checks.same(List.of(example.EventFixture.commandArgs), List.of("spaced", "out"),
            "runs of spaces collapse");

        // An alias from plugin.yml reaches the same command, and the handler
        // is told the label that was actually typed.
        Checks.expect(foton.CommandMap.dispatch(player, "fx aliased"),
            "an alias should reach the command");
        Checks.same(example.EventFixture.commandLabel, "fx", "the handler sees the typed label");

        // Returning false prints the usage line, with <command> replaced.
        player.clear();
        Checks.expect(foton.CommandMap.dispatch(player, "fixture"),
            "a command that declined its arguments is still owned");
        Checks.same(player.last(), "/fixture <word>", "the usage line from plugin.yml");

        permissions();
    }

    /** A command with a permission asks before it runs anything. */
    private static void permissions() {
        example.EventFixture.commandLabel = null;

        Recorder denied = new Recorder("Mallory");
        denied.allowed = false;
        Checks.expect(foton.CommandMap.dispatch(denied, "guarded now"),
            "a refused command is still owned: the server must not call it unknown");
        Checks.same(denied.last(), "You may not do that.",
            "the permission-message from plugin.yml");
        Checks.same(example.EventFixture.commandLabel, null,
            "a refused command must not reach the handler");

        Recorder allowed = new Recorder("Ada");
        Checks.expect(foton.CommandMap.dispatch(allowed, "guarded now"),
            "a permitted command runs");
        Checks.same(example.EventFixture.commandLabel, "guarded",
            "a permitted command reaches the handler");
    }

    /** A sender that keeps what it was told, so the check can read it. */
    private static final class Recorder implements CommandSender {
        private final String name;
        private final List<String> messages = new ArrayList<>();
        boolean allowed = true;

        Recorder(String name) {
            this.name = name;
        }

        @Override public void sendMessage(String message) {
            messages.add(message);
        }

        @Override public boolean hasPermission(String permission) {
            return allowed;
        }

        @Override public String getName() {
            return name;
        }

        String last() {
            return messages.isEmpty() ? null : messages.get(messages.size() - 1);
        }

        void clear() {
            messages.clear();
        }
    }
}
