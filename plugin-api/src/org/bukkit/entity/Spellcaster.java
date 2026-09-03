package org.bukkit.entity;

/** An illager capable of casting a spell. */
public interface Spellcaster extends LivingEntity {
    enum Spell { NONE, SUMMON_VEX, FANGS, WOLOLO, DISAPPEAR, BLINDNESS }
    Spell getSpell();
    void setSpell(Spell spell);
}
