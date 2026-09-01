package foton;

import org.bukkit.command.CommandSender;

/** The console, as a plugin's command handler sees it. */
public final class ConsoleSender implements CommandSender {
    public static final ConsoleSender INSTANCE = new ConsoleSender();

    private ConsoleSender() {}

    @Override
    public void sendMessage(String message) {
        System.out.println(message);
    }

    /** The console may do anything, which is what every server assumes. */
    @Override
    public boolean hasPermission(String permission) {
        return true;
    }

    @Override
    public String getName() {
        return "CONSOLE";
    }
}
