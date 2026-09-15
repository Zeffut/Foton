package example;

import java.sql.Connection;
import java.sql.Driver;
import java.sql.DriverManager;
import java.sql.DriverPropertyInfo;
import java.sql.SQLException;
import java.util.Properties;
import java.util.logging.Logger;

/** Must never be process-globally discovered by the host. */
public final class PrivateDriver implements Driver {
    static {
        System.setProperty("foton.fixture.privateDriverRegistrations",
            Integer.toString(Integer.getInteger(
                "foton.fixture.privateDriverRegistrations", 0) + 1));
        try {
            DriverManager.registerDriver(new PrivateDriver());
        } catch (SQLException error) {
            throw new ExceptionInInitializerError(error);
        }
    }

    @Override public Connection connect(String url, Properties info) { return null; }
    @Override public boolean acceptsURL(String url) { return false; }
    @Override public DriverPropertyInfo[] getPropertyInfo(String url, Properties info) {
        return new DriverPropertyInfo[0];
    }
    @Override public int getMajorVersion() { return 1; }
    @Override public int getMinorVersion() { return 0; }
    @Override public boolean jdbcCompliant() { return false; }
    @Override public Logger getParentLogger() { return Logger.getGlobal(); }
}
