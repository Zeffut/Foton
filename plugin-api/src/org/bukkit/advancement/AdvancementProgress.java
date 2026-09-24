package org.bukkit.advancement;

import java.util.Collection;
import java.util.Date;

/** One player's progress on one advancement. */
public interface AdvancementProgress {
    Advancement getAdvancement();
    boolean isDone();
    boolean awardCriteria(String criteria);
    boolean revokeCriteria(String criteria);
    Date getDateAwarded(String criteria);
    Collection<String> getRemainingCriteria();
    Collection<String> getAwardedCriteria();
}
