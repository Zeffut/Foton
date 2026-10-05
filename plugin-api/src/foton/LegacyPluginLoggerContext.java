package foton;

/** Preserves the original Foton logger JVM descriptor through a covariant bridge. */
public interface LegacyPluginLoggerContext {
    org.slf4j.Logger getLogger();
}
