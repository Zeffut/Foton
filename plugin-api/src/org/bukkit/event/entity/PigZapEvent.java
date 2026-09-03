package org.bukkit.event.entity;
import com.destroystokyo.paper.event.entity.EntityZapEvent;
import org.bukkit.entity.LightningStrike;
import org.bukkit.entity.Pig;
import org.bukkit.entity.PigZombie;
/** @deprecated use EntityZapEvent. */
@Deprecated
public class PigZapEvent extends EntityZapEvent {
 public PigZapEvent(Pig pig, LightningStrike bolt, PigZombie zombifiedPiglin){super(pig,bolt,zombifiedPiglin);}
 @Override public Pig getEntity(){return (Pig) super.getEntity();}
 public LightningStrike getLightning(){return super.getBolt();}
 public PigZombie getPigZombie(){return (PigZombie) super.getReplacementEntity();}
}
