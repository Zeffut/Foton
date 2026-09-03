package foton;

import java.util.List;
import java.util.UUID;
import org.bukkit.inventory.MerchantRecipe;

/** Common Bukkit surface for villager-like merchant entities. */
public abstract class AbstractVillager extends FotonLivingEntity implements org.bukkit.entity.AbstractVillager {
    protected AbstractVillager(UUID id) { super(id); }
    public abstract List<MerchantRecipe> getRecipes();
}
