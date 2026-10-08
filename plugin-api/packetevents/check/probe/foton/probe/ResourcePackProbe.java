package foton.probe;

import java.net.URI;
import java.util.Map;
import java.util.UUID;
import java.util.concurrent.atomic.AtomicInteger;
import net.kyori.adventure.resource.ResourcePackCallback;
import net.kyori.adventure.resource.ResourcePackInfo;
import net.kyori.adventure.resource.ResourcePackRequest;
import net.kyori.adventure.text.Component;
import org.bukkit.command.Command;
import org.bukkit.command.CommandExecutor;
import org.bukkit.command.CommandSender;
import org.bukkit.entity.Player;
import org.bukkit.event.EventHandler;
import org.bukkit.event.Listener;
import org.bukkit.event.player.PlayerResourcePackStatusEvent;

/** Writes down every resource pack status the server reports, and pushes or
 * pops packs on request:
 *
 * <pre>
 * /proberp set URL SHA1 FORCE         Player.setResourcePack(url, hash, force)
 * /proberp add UUID URL SHA1          sendResourcePacks, with a callback
 * /proberp remove UUID                Player.removeResourcePack
 * /proberp clear                      Player.clearResourcePacks
 * /proberp status                     getResourcePackStatus / hasResourcePack
 * </pre>
 */
final class ResourcePackProbe implements Listener, CommandExecutor {
    private final Map<String, String> facts;
    private final AtomicInteger statuses = new AtomicInteger();
    private final AtomicInteger callbacks = new AtomicInteger();

    ResourcePackProbe(Map<String, String> facts) {
        this.facts = facts;
    }

    @EventHandler
    public void onStatus(PlayerResourcePackStatusEvent event) {
        Player player = event.getPlayer();
        facts.put("pack status " + statuses.incrementAndGet(), player.getName() + " " + event.getID()
            + " " + event.getStatus() + " (player says " + player.getResourcePackStatus()
            + ", loaded " + player.hasResourcePack() + ")");
    }

    @Override
    public boolean onCommand(CommandSender sender, Command command, String label, String[] args) {
        if (!(sender instanceof Player player) || args.length == 0) return true;
        try {
            switch (args[0]) {
                case "set" -> player.setResourcePack(args[1], args[2].equals("-") ? "" : args[2], Boolean.parseBoolean(args[3]));
                case "add" -> player.sendResourcePacks(ResourcePackRequest.resourcePackRequest()
                    .packs(ResourcePackInfo.resourcePackInfo(UUID.fromString(args[1]), URI.create(args[2]),
                        args[3].equals("-") ? "" : args[3]))
                    .prompt(Component.text("probe pack"))
                    .callback((ResourcePackCallback) (id, status, audience) ->
                        facts.put("pack callback " + callbacks.incrementAndGet(), id + " " + status
                            + " audience " + (audience instanceof Player)))
                    .build());
                case "remove" -> player.removeResourcePack(UUID.fromString(args[1]));
                case "clear" -> player.clearResourcePacks();
                case "status" -> facts.put("pack status now", player.getResourcePackStatus() + " loaded "
                    + player.hasResourcePack());
                default -> facts.put("pack command", "unknown " + args[0]);
            }
        } catch (Throwable error) {
            facts.put("pack command " + args[0], "threw " + error);
        }
        return true;
    }
}
