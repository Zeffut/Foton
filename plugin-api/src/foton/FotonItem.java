package foton;

import java.util.UUID;
import org.bukkit.entity.Item;

/** Item entity handle backed by a UUID. */
public final class FotonItem extends FotonEntity implements Item {
    public FotonItem(UUID id) { super(id); }
}
