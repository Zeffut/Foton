package foton;

import org.bukkit.command.ConsoleCommandSender;

/** The console, as a plugin's command handler sees it. */
public final class ConsoleSender implements ConsoleCommandSender {
    public static final ConsoleSender INSTANCE = new ConsoleSender();

    private ConsoleSender() {}

    @Override
    public void sendMessage(String message) {
        System.out.println(message);
    }

    /** The console is an operator, while explicitly false permissions stay denied. */
    @Override
    public boolean hasPermission(String permission) {
        if (permission == null) return false;
        Boolean declared = PermissionRegistry.resolveDefault(permission, true);
        return declared == null || declared;
    }

    @Override
    public boolean isOp() { return true; }

    @Override
    public String getName() {
        return "CONSOLE";
    }
}
