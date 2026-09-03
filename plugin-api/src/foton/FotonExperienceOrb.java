package foton;

import java.util.UUID;
import org.bukkit.entity.ExperienceOrb;

public final class FotonExperienceOrb extends FotonEntity implements ExperienceOrb {
    public FotonExperienceOrb(UUID id) { super(id); }
    @Override public int getExperience() { return Native.experienceOrbExperience(getUniqueId().toString()); }
    @Override public void setExperience(int experience) { Native.setExperienceOrbExperience(getUniqueId().toString(), experience); }
}
