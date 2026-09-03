package foton;

import java.util.UUID;

/** Offline owner handle returned by tameable entities. */
final class FotonAnimalTamer extends FotonOfflinePlayer implements org.bukkit.entity.AnimalTamer {
    FotonAnimalTamer(UUID id, String name) { super(id, name); }
}
