package foton;

import java.util.UUID;

/** Live Bukkit view of an interaction entity. */
public final class FotonInteraction extends FotonEntity implements org.bukkit.entity.Interaction {
    public FotonInteraction(UUID id) { super(id); }

    private double state(int index, double otherwise) {
        double[] state = Native.interactionState(getUniqueId().toString());
        return state == null || state.length < 3 ? otherwise : state[index];
    }

    @Override public float getInteractionWidth() { return (float) state(0, 1.0); }
    @Override public void setInteractionWidth(float width) { Native.setInteractionValue(getUniqueId().toString(), 0, width); }
    @Override public float getInteractionHeight() { return (float) state(1, 1.0); }
    @Override public void setInteractionHeight(float height) { Native.setInteractionValue(getUniqueId().toString(), 1, height); }
    @Override public boolean isResponsive() { return state(2, 0) != 0; }
    @Override public void setResponsive(boolean response) { Native.setInteractionValue(getUniqueId().toString(), 2, response ? 1.0f : 0.0f); }
}
