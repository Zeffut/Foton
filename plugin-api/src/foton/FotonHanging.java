package foton;

import java.util.UUID;
import org.bukkit.entity.Hanging;

/** UUID-backed wrapper used for Bukkit hanging-entity events. */
public final class FotonHanging extends FotonEntity implements Hanging {
    public FotonHanging(UUID id) { super(id); }
}
