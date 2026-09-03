package foton;

import java.util.UUID;

/** Live Bukkit view of a Steel spellcaster. */
public final class FotonSpellcaster extends FotonLivingEntity implements org.bukkit.entity.Spellcaster {
    public FotonSpellcaster(UUID id) { super(id); }
    @Override public Spell getSpell() {
        String value = Native.spellcasterSpell(getUniqueId().toString());
        try { return value == null ? Spell.NONE : Spell.valueOf(value); }
        catch (IllegalArgumentException error) { return Spell.NONE; }
    }
    @Override public void setSpell(Spell spell) { Native.setSpellcasterSpell(getUniqueId().toString(), spell == null ? "NONE" : spell.name()); }
}
