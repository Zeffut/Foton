package foton;
import java.util.UUID;
public final class FotonLlama extends FotonLivingEntity implements org.bukkit.entity.Llama {
    public FotonLlama(UUID id) { super(id); }
    @Override public boolean isCarryingChest() { return Native.entityHasChest(getUniqueId().toString()); }
    @Override public void setCarryingChest(boolean value) { Native.entitySetChest(getUniqueId().toString(), value); }
    @Override public org.bukkit.inventory.LlamaInventory getInventory() { return new FotonHorseInventory(getUniqueId().toString()); }
    @Override public Color getColor() { String v=Native.llamaVariant(getUniqueId().toString()); if(v==null)return Color.CREAMY; try{return Color.valueOf(v.toUpperCase(java.util.Locale.ROOT));}catch(IllegalArgumentException e){return Color.CREAMY;} }
    @Override public void setColor(Color value) { if(value!=null) Native.setLlamaVariant(getUniqueId().toString(), value.name()); }
}
