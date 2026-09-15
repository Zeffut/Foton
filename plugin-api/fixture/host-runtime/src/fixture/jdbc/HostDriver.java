package fixture.jdbc;

import java.lang.reflect.Proxy;
import java.sql.Connection;
import java.sql.Driver;
import java.sql.DriverManager;
import java.sql.DriverPropertyInfo;
import java.sql.SQLException;
import java.util.Properties;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.logging.Logger;

/** A host-classpath JDBC provider used to exercise DriverManager discovery. */
public final class HostDriver implements Driver {
    private static final AtomicInteger CONNECTIONS = new AtomicInteger();

    static {
        try {
            DriverManager.registerDriver(new HostDriver());
        } catch (SQLException error) {
            throw new ExceptionInInitializerError(error);
        }
    }

    public static int connections() {
        return CONNECTIONS.get();
    }

    public static long registeredDrivers() {
        return DriverManager.drivers()
            .filter(driver -> driver.getClass() == HostDriver.class)
            .count();
    }

    @Override
    public Connection connect(String url, Properties info) throws SQLException {
        if (!acceptsURL(url)) return null;
        CONNECTIONS.incrementAndGet();
        return (Connection) Proxy.newProxyInstance(
            HostDriver.class.getClassLoader(),
            new Class<?>[] { Connection.class },
            (proxy, method, args) -> switch (method.getName()) {
                case "close" -> null;
                case "isClosed" -> false;
                case "isWrapperFor" -> false;
                case "unwrap" -> throw new SQLException("not a wrapper");
                case "toString" -> "HostFixtureConnection";
                default -> throw new UnsupportedOperationException(method.getName());
            });
    }

    @Override public boolean acceptsURL(String url) {
        return url != null && url.startsWith("jdbc:foton-host:");
    }

    @Override public DriverPropertyInfo[] getPropertyInfo(String url, Properties info) {
        return new DriverPropertyInfo[0];
    }

    @Override public int getMajorVersion() { return 1; }
    @Override public int getMinorVersion() { return 0; }
    @Override public boolean jdbcCompliant() { return false; }
    @Override public Logger getParentLogger() { return Logger.getGlobal(); }
}
