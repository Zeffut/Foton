package foton;

import java.util.UUID;

/** The distinct wrapper returned only by a TippedArrow class spawn. */
@SuppressWarnings("deprecation")
public final class FotonTippedArrow extends FotonArrow implements org.bukkit.entity.TippedArrow {
    public FotonTippedArrow(UUID id) { super(id); }
}
