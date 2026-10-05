package foton;

import com.mojang.brigadier.CommandDispatcher;
import com.mojang.brigadier.builder.LiteralArgumentBuilder;

/** A cycle must never mutate retained bootstrap commands or redirect into stale trees. */
public final class BootstrapCommandSeedCheck {
    private BootstrapCommandSeedCheck() {}
    public static void main(String[] args) throws Exception { check(); }
    public static void check() throws Exception {
        CommandDispatcher<Object> staged = new CommandDispatcher<>();
        var bootstrap = staged.register(LiteralArgumentBuilder.<Object>literal("boot").executes(context -> 7));
        staged.register(LiteralArgumentBuilder.<Object>literal("alias").redirect(bootstrap));
        staged.register(LiteralArgumentBuilder.<Object>literal("root").redirect(staged.getRoot()));
        CommandDispatcher<Object> first = new CommandDispatcher<>();
        CommandTreeSeed.copyInto(staged.getRoot(), first.getRoot());
        first.register(LiteralArgumentBuilder.<Object>literal("boot")
            .then(LiteralArgumentBuilder.<Object>literal("cycle").executes(context -> 9)));
        if (staged.getRoot().getChild("boot").getChild("cycle") != null)
            throw new AssertionError("enable-cycle mutation leaked into staged bootstrap tree");
        same(first.execute("alias cycle", new Object()), 9);
        same(first.execute("root boot cycle", new Object()), 9);
        CommandDispatcher<Object> second = new CommandDispatcher<>();
        CommandTreeSeed.copyInto(staged.getRoot(), second.getRoot());
        same(second.execute("boot", new Object()), 7);
        if (second.getRoot().getChild("boot").getChild("cycle") != null)
            throw new AssertionError("prior-cycle child leaked into re-enable");
        if (second.getRoot().getChild("alias").getRedirect() != second.getRoot().getChild("boot"))
            throw new AssertionError("redirect retained a stale owner tree");
    }
    private static void same(int actual, int expected) {
        if (actual != expected) throw new AssertionError("expected " + expected + ", got " + actual);
    }
}
