package org.bukkit.help;
import java.util.Collection;
public interface HelpMap {
    HelpTopic getHelpTopic(String topicName);
    Collection<HelpTopic> getHelpTopics();
    <T extends org.bukkit.command.Command> void registerHelpTopicFactory(Class<T> commandClass, HelpTopicFactory<T> factory);
}
