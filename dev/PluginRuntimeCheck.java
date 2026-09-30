import com.google.common.util.concurrent.Futures;
import com.google.common.util.concurrent.SettableFuture;

/** Executable linkage check for the server-provided plugin classpath. */
public final class PluginRuntimeCheck {
    private PluginRuntimeCheck() {}

    public static void main(String[] args) throws Exception {
        try (java.sql.Connection connection = java.sql.DriverManager.getConnection("jdbc:sqlite::memory:");
                java.sql.Statement statement = connection.createStatement();
                java.sql.ResultSet result = statement.executeQuery("select 31")) {
            if (!result.next() || result.getInt(1) != 31) {
                throw new AssertionError("SQLite JDBC did not execute a query");
            }
        }
        SettableFuture<String> future = SettableFuture.create();
        future.set("linked");
        if (!"linked".equals(Futures.getDone(future))) {
            throw new AssertionError("Guava future did not complete");
        }
    }
}
